use anyhow::{Context, Result};
use std::{
    env,
    fs::{self, File, OpenOptions},
    hint::black_box,
    io::{BufWriter, Write},
    path::PathBuf,
    thread,
    time::Duration,
};
use tracing::info;
use tracing_subscriber::EnvFilter;
use votor_measurement_harness::{
    banking_stage_metrics::BankingStageMetrics,
    banking_stage_trace::{BankingLatencyRecord, EnqueueTimestamp, StageTimer},
};

#[derive(Clone, Copy, Debug)]
enum Profile {
    Baseline,
    CpuSaturation,
    IoStall,
    AllocatorPressure,
}

impl Profile {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "baseline" => Ok(Self::Baseline),
            "cpu_saturation" => Ok(Self::CpuSaturation),
            "io_stall" => Ok(Self::IoStall),
            "allocator_pressure" => Ok(Self::AllocatorPressure),
            other => anyhow::bail!(
                "unknown profile `{other}`; expected baseline, cpu_saturation, io_stall, allocator_pressure"
            ),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::CpuSaturation => "cpu_saturation",
            Self::IoStall => "io_stall",
            Self::AllocatorPressure => "allocator_pressure",
        }
    }

    fn default_iterations(self) -> u64 {
        match self {
            Self::Baseline => 200,
            Self::CpuSaturation => 200,
            Self::IoStall => 120,
            Self::AllocatorPressure => 160,
        }
    }

    fn batch_size(self) -> usize {
        match self {
            Self::Baseline => 128,
            Self::CpuSaturation => 128,
            Self::IoStall => 96,
            Self::AllocatorPressure => 192,
        }
    }
}

fn main() -> Result<()> {
    init_tracing();

    let config = Config::from_env_and_args()?;
    let output_path = config.output_path();
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }

    let output = File::create(&output_path)
        .with_context(|| format!("creating trace output {}", output_path.display()))?;
    let mut writer = BufWriter::new(output);
    let metrics = BankingStageMetrics::new();
    let mut cpu_workers = Vec::new();

    if matches!(config.profile, Profile::CpuSaturation) {
        cpu_workers = spawn_cpu_pressure_workers();
    }

    let io_path = output_path.with_extension("io-stall.tmp");
    let mut io_file = if matches!(config.profile, Profile::IoStall) {
        Some(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&io_path)
                .with_context(|| format!("opening {}", io_path.display()))?,
        )
    } else {
        None
    };

    info!(
        profile = config.profile.as_str(),
        iterations = config.iterations,
        "starting banking jitter harness"
    );

    for slot in 0..config.iterations {
        let record = run_batch(config.profile, slot, &metrics, io_file.as_mut())?;
        serde_json::to_writer(&mut writer, &record)?;
        writer.write_all(b"\n")?;
    }

    drop(cpu_workers);
    writer.flush()?;
    info!("finished banking jitter harness");
    Ok(())
}

#[cfg(feature = "banking-jitter-console")]
fn init_tracing() {
    console_subscriber::init();
}

#[cfg(not(feature = "banking-jitter-console"))]
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

#[derive(Debug)]
struct Config {
    profile: Profile,
    iterations: u64,
    output_path: PathBuf,
}

impl Config {
    fn from_env_and_args() -> Result<Self> {
        let mut profile = env::var("BANKING_JITTER_PROFILE").unwrap_or_else(|_| "baseline".into());
        let mut iterations = None;
        let mut output_path = env::var_os("BANKING_JITTER_OUT").map(PathBuf::from);

        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--profile" => {
                    profile = args.next().context("--profile requires a value")?;
                }
                "--iterations" => {
                    let value = args.next().context("--iterations requires a value")?;
                    iterations = Some(value.parse::<u64>().context("parsing --iterations")?);
                }
                "--out" => {
                    output_path = Some(PathBuf::from(
                        args.next().context("--out requires a value")?,
                    ));
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                other => anyhow::bail!("unknown argument `{other}`"),
            }
        }

        let profile = Profile::parse(&profile)?;
        let output_path = output_path.unwrap_or_else(|| {
            PathBuf::from(format!(
                "artifacts/banking-jitter/{}/trace.json",
                profile.as_str()
            ))
        });

        Ok(Self {
            profile,
            iterations: iterations.unwrap_or_else(|| profile.default_iterations()),
            output_path,
        })
    }

    fn output_path(&self) -> PathBuf {
        self.output_path.clone()
    }
}

fn print_help() {
    println!(
        "Usage: votor-measurement-harness [--profile baseline|cpu_saturation|io_stall|allocator_pressure] [--iterations N] [--out PATH]"
    );
}

fn run_batch(
    profile: Profile,
    slot: u64,
    metrics: &BankingStageMetrics,
    io_file: Option<&mut File>,
) -> Result<BankingLatencyRecord> {
    let profile_name = profile.as_str();
    let batch_size = profile.batch_size();
    let total = StageTimer::start("total_batch", profile_name, batch_size);
    let enqueued_at = EnqueueTimestamp::now();

    simulate_queue_wait(profile, slot);

    let dequeue_timer = StageTimer::start("dequeue_work", profile_name, batch_size);
    let queue_delay_us = enqueued_at.elapsed_us();
    dequeue_timer.record_queue_delay(queue_delay_us);
    simulate_dequeue(profile);
    let _dequeue_us = dequeue_timer.finish();
    metrics.record_queue_delay(queue_delay_us);

    let build_timer = StageTimer::start("build_batch", profile_name, batch_size);
    simulate_build_batch(profile, batch_size);
    let build_batch_us = build_timer.finish();
    metrics.record_build_batch(build_batch_us);

    let lock_timer = StageTimer::start("account_lock_and_schedule", profile_name, batch_size);
    simulate_lock_and_schedule(profile, batch_size);
    let lock_us = lock_timer.finish();
    metrics.record_lock(lock_us);

    let exec_timer = StageTimer::start("execute_transactions", profile_name, batch_size);
    simulate_execution(profile, batch_size);
    let execution_us = exec_timer.finish();
    metrics.record_execution(execution_us);

    let record_timer = StageTimer::start("record_results", profile_name, batch_size);
    simulate_record_results(profile, slot, io_file)?;
    let record_us = record_timer.finish();
    metrics.record_results(record_us);

    let total_us = total.finish();
    metrics.record_batch();

    Ok(BankingLatencyRecord {
        profile: profile_name.to_string(),
        batch_size,
        queue_delay_us,
        build_batch_us,
        lock_us,
        execution_us,
        record_us,
        total_us,
        slot,
        thread: thread::current().name().unwrap_or("main").to_string(),
    })
}

fn simulate_queue_wait(profile: Profile, slot: u64) {
    let delay = match profile {
        Profile::Baseline => Duration::from_micros(250),
        Profile::CpuSaturation => Duration::from_micros(if slot % 20 == 0 { 2_500 } else { 500 }),
        Profile::IoStall => Duration::from_micros(400),
        Profile::AllocatorPressure => Duration::from_micros(500),
    };
    thread::sleep(delay);
}

fn simulate_dequeue(profile: Profile) {
    let spins = match profile {
        Profile::Baseline => 1_000,
        Profile::CpuSaturation => 5_000,
        Profile::IoStall => 1_500,
        Profile::AllocatorPressure => 2_000,
    };
    cpu_work(spins);
}

fn simulate_build_batch(profile: Profile, batch_size: usize) {
    match profile {
        Profile::AllocatorPressure => {
            let chunks_len = batch_size * 8;
            let mut chunks = Vec::with_capacity(chunks_len);
            for idx in 0..chunks_len {
                chunks.push(vec![idx as u8; 4096]);
            }
            black_box(chunks);
        }
        _ => cpu_work(batch_size as u64 * 200),
    }
}

fn simulate_lock_and_schedule(profile: Profile, batch_size: usize) {
    let multiplier = match profile {
        Profile::CpuSaturation => 700,
        Profile::AllocatorPressure => 500,
        _ => 300,
    };
    cpu_work(batch_size as u64 * multiplier);
}

fn simulate_execution(profile: Profile, batch_size: usize) {
    let multiplier = match profile {
        Profile::CpuSaturation => 1_600,
        Profile::AllocatorPressure => 900,
        Profile::IoStall => 700,
        Profile::Baseline => 650,
    };
    cpu_work(batch_size as u64 * multiplier);
}

fn simulate_record_results(profile: Profile, slot: u64, io_file: Option<&mut File>) -> Result<()> {
    if let (Profile::IoStall, Some(file)) = (profile, io_file) {
        writeln!(file, "slot={slot}")?;
        if slot % 5 == 0 {
            file.flush()?;
            thread::sleep(Duration::from_millis(2));
        }
    } else {
        cpu_work(1_000);
    }
    Ok(())
}

fn cpu_work(iterations: u64) {
    let mut acc = 0_u64;
    for value in 0..iterations {
        acc = acc.wrapping_add(value.rotate_left((value % 17) as u32));
    }
    black_box(acc);
}

fn spawn_cpu_pressure_workers() -> Vec<thread::JoinHandle<()>> {
    let workers = thread::available_parallelism()
        .map(|count| count.get().saturating_sub(1).min(4))
        .unwrap_or(1);

    (0..workers)
        .map(|idx| {
            thread::Builder::new()
                .name(format!("cpu-pressure-{idx}"))
                .spawn(|| {
                    for _ in 0..300 {
                        cpu_work(25_000);
                        thread::yield_now();
                    }
                })
                .expect("spawn cpu pressure worker")
        })
        .collect()
}
