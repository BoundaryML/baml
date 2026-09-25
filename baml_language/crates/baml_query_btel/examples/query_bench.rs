//! Same-process query benchmark, run by `tools/btel_query_bench.py`.
//!
//! `PROJECT ITERATIONS SQL...`: opens one `Index` for `PROJECT` and keeps it,
//! as the playground does. Reports the first refresh, then for each SQL
//! statement its first run and `ITERATIONS` repeated refresh-and-query runs
//! against an unchanged source. Prints one JSON line.
//!
//! Examples link the crate's `fault-injection` dev feature: each refresh
//! checks an unset environment variable at a few transaction boundaries.
#![allow(clippy::print_stdout, clippy::cast_precision_loss)]
use std::{path::Path, time::Instant};

use baml_query_btel::{Index, IndexOptions, QueryRequest};
use serde_json::{Value, json};

fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1e3
}

/// Peak resident set size in bytes, from `/proc/self/status`.
fn peak_rss_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("VmHWM:"))?;
    let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kib * 1024)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    assert!(args.len() >= 4, "PROJECT ITERATIONS SQL...");
    let project = Path::new(&args[1]);
    let iterations: usize = args[2].parse().expect("ITERATIONS");

    let open = Instant::now();
    let mut index = Index::for_project(project, IndexOptions::default()).expect("open index");
    let open_ms = ms(open);
    let first_refresh = index.refresh().expect("first refresh");

    let mut queries = Vec::new();
    for sql in &args[3..] {
        let request = QueryRequest {
            sql: sql.clone(),
            ..QueryRequest::default()
        };
        let first = index.query(&request).expect("first query");
        let mut runs = Vec::with_capacity(iterations);
        for _ in 0..iterations {
            let started = Instant::now();
            let result = index.refresh_and_query(&request).expect("repeated query");
            let wall_ms = ms(started);
            let refresh = result.outcome.refresh.expect("refresh metrics");
            assert_eq!(refresh.files_decoded, 0, "unchanged source decoded a file");
            assert_eq!(refresh.bytes_read, 0, "unchanged source read a file");
            assert_eq!(refresh.transactions, 0, "unchanged source wrote the index");
            assert_eq!(result.rows, first.rows, "repeated query changed rows");
            runs.push(json!({
                "wall_ms": wall_ms,
                "refresh_ms": refresh.total_ms,
                "query": result.outcome.query,
            }));
        }
        queries.push(json!({
            "sql": sql,
            "rows": first.rows.len(),
            "result_rows": first.rows,
            "status": first.outcome.status,
            "first": first.outcome.query,
            "repeated": Value::Array(runs),
        }));
    }
    println!(
        "{}",
        json!({
            "open_ms": open_ms,
            "first_refresh": first_refresh,
            "queries": queries,
            "peak_rss_bytes": peak_rss_bytes(),
        })
    );
}
