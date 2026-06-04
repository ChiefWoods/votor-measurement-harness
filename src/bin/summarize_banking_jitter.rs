use anyhow::{Context, Result};
use serde::Deserialize;
use std::{env, fs, path::PathBuf};

#[derive(Debug, Deserialize)]
struct BankingLatencyRecord {
    profile: String,
    queue_delay_us: u64,
    build_batch_us: u64,
    lock_us: u64,
    execution_us: u64,
    record_us: u64,
    total_us: u64,
}

#[derive(Debug)]
struct Summary {
    profile: String,
    count: usize,
    total_p50_ms: Option<f64>,
    total_p95_ms: Option<f64>,
    total_p99_ms: Option<f64>,
    total_max_ms: Option<f64>,
    queue_p95_ms: Option<f64>,
    build_p95_ms: Option<f64>,
    lock_p95_ms: Option<f64>,
    execution_p95_ms: Option<f64>,
    record_p95_ms: Option<f64>,
}

fn main() -> Result<()> {
    let paths = env::args().skip(1).map(PathBuf::from).collect::<Vec<_>>();
    if paths.is_empty() {
        anyhow::bail!("usage: summarize_banking_jitter <trace.json> [trace.json ...]");
    }

    let summaries = paths
        .iter()
        .map(summarize_path)
        .collect::<Result<Vec<_>>>()?;

    let markdown = render_markdown(&summaries);
    fs::create_dir_all("artifacts/banking-jitter")?;
    fs::write("artifacts/banking-jitter/summary.md", &markdown)?;
    println!("{markdown}");

    Ok(())
}

fn summarize_path(path: &PathBuf) -> Result<Summary> {
    let input = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let rows = input
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str::<BankingLatencyRecord>)
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("parsing {}", path.display()))?;

    let profile = rows
        .first()
        .map(|row| row.profile.clone())
        .unwrap_or_else(|| path.display().to_string());

    let mut total = rows.iter().map(|row| row.total_us).collect::<Vec<_>>();
    let mut queue = rows
        .iter()
        .map(|row| row.queue_delay_us)
        .collect::<Vec<_>>();
    let mut build = rows
        .iter()
        .map(|row| row.build_batch_us)
        .collect::<Vec<_>>();
    let mut lock = rows.iter().map(|row| row.lock_us).collect::<Vec<_>>();
    let mut execution = rows.iter().map(|row| row.execution_us).collect::<Vec<_>>();
    let mut record = rows.iter().map(|row| row.record_us).collect::<Vec<_>>();
    let total_max_ms = total.iter().max().map(|value| *value as f64 / 1000.0);

    Ok(Summary {
        profile,
        count: rows.len(),
        total_p50_ms: percentile_ms(&mut total.clone(), 0.50),
        total_p95_ms: percentile_ms(&mut total.clone(), 0.95),
        total_p99_ms: percentile_ms(&mut total, 0.99),
        total_max_ms,
        queue_p95_ms: percentile_ms(&mut queue, 0.95),
        build_p95_ms: percentile_ms(&mut build, 0.95),
        lock_p95_ms: percentile_ms(&mut lock, 0.95),
        execution_p95_ms: percentile_ms(&mut execution, 0.95),
        record_p95_ms: percentile_ms(&mut record, 0.95),
    })
}

fn percentile_ms(values: &mut [u64], q: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let idx = ((values.len() - 1) as f64 * q).round() as usize;
    Some(values[idx] as f64 / 1000.0)
}

fn render_markdown(summaries: &[Summary]) -> String {
    let mut markdown = String::from("# Banking Jitter Summary\n\n");
    markdown.push_str("| Profile | Count | Total p50 ms | Total p95 ms | Total p99 ms | Total max ms | Queue p95 ms | Build p95 ms | Lock p95 ms | Execution p95 ms | Record p95 ms |\n");
    markdown.push_str(
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n",
    );

    for summary in summaries {
        markdown.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            summary.profile,
            summary.count,
            fmt(summary.total_p50_ms),
            fmt(summary.total_p95_ms),
            fmt(summary.total_p99_ms),
            fmt(summary.total_max_ms),
            fmt(summary.queue_p95_ms),
            fmt(summary.build_p95_ms),
            fmt(summary.lock_p95_ms),
            fmt(summary.execution_p95_ms),
            fmt(summary.record_p95_ms),
        ));
    }

    markdown
}

fn fmt(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.3}"))
        .unwrap_or_else(|| "n/a".to_string())
}
