//! `obs-bench replay` — time the artifact open/reconstruct path (design
//! §10.3; the C6 legacy-side measurement).
//!
//! C6's claim: the ingest/open path that replaces the quadratic run store
//! opens a 4,096-event artifact in ≤250 ms and scales linearly. Until
//! `bex_query` lands (P4), the measured candidate is today's
//! `reconstruct_bamlprof` path — the baseline the new engine is judged
//! against; the subcommand grows an `--engine bexq` leg in P4.

use std::time::Instant;

use anyhow::Context as _;
use bex_events::{prof::read::read_bamlprof_from_bytes, run::bamlprof::reconstruct_bamlprof};

use crate::rows::{Basis, BenchRow};

pub struct ReplayReport {
    pub rows: Vec<BenchRow>,
    pub summary: String,
}

/// Parse + reconstruct each artifact, timing both phases.
pub fn run(paths: &[std::path::PathBuf], pipeline: &str) -> anyhow::Result<ReplayReport> {
    let files = crate::prof_stats::collect_files(paths, "bamlprof")?;
    anyhow::ensure!(!files.is_empty(), "no .bamlprof files under {paths:?}");

    let mut rows = Vec::new();
    let mut summary = String::new();
    for file in files {
        let bytes = std::fs::read(&file).with_context(|| format!("reading {}", file.display()))?;

        let parse_start = Instant::now();
        let contents = read_bamlprof_from_bytes(&bytes)
            .with_context(|| format!("parsing {}", file.display()))?;
        let parse_ms = parse_start.elapsed().as_secs_f64() * 1e3;

        let reconstruct_start = Instant::now();
        let profile = reconstruct_bamlprof(&contents)
            .map_err(|e| anyhow::anyhow!("reconstructing {}: {e:?}", file.display()))?;
        let reconstruct_ms = reconstruct_start.elapsed().as_secs_f64() * 1e3;

        let events = contents.events.len() as f64;
        let workload = file
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "artifact".to_string());

        use std::fmt::Write as _;
        let _ = writeln!(
            summary,
            "{}: {} events, parse {:.2} ms, reconstruct {:.2} ms, {} calls, {} threads, {} diagnostics",
            file.display(),
            contents.events.len(),
            parse_ms,
            reconstruct_ms,
            profile.calls.len(),
            profile.threads.len(),
            profile.diagnostics.len(),
        );

        rows.push(
            BenchRow::new(
                "c6.replay.parse_ms",
                &workload,
                pipeline,
                "parse_ms",
                parse_ms,
                "ms",
                Basis::Measured,
            )
            .with_notes(format!("events={events}")),
        );
        rows.push(
            BenchRow::new(
                "c6.replay.reconstruct_ms",
                &workload,
                pipeline,
                "reconstruct_ms",
                reconstruct_ms,
                "ms",
                Basis::Measured,
            )
            .with_notes(format!("events={events}")),
        );
        rows.push(BenchRow::new(
            "c6.replay.events",
            &workload,
            pipeline,
            "events",
            events,
            "count",
            Basis::Measured,
        ));
    }
    Ok(ReplayReport { rows, summary })
}

/// C6 engine leg (P4): time `bex_query`'s open→first-frame path over a v2
/// `.baml` root — `ObserveEngine::open_run` (cold fold from disk) plus the
/// first `left_heavy` frame, per run key. The C6 gate: a 4,096-event
/// artifact opens in ≤250 ms; scaling stays linear.
pub fn run_v2(root: &std::path::Path) -> anyhow::Result<ReplayReport> {
    use std::fmt::Write as _;
    let mut engine = bex_query::ObserveEngine::new(root.to_path_buf());
    let mut keys: Vec<String> = Vec::new();
    for sub in ["history", "sessions"] {
        if let Ok(entries) = std::fs::read_dir(root.join(sub)) {
            for e in entries.filter_map(Result::ok) {
                if e.path().is_dir() && e.file_name() != "_unbound" {
                    keys.push(e.file_name().to_string_lossy().into_owned());
                }
            }
        }
    }
    anyhow::ensure!(
        !keys.is_empty(),
        "no runs/sessions under {}",
        root.display()
    );

    let mut rows = Vec::new();
    let mut summary = String::new();
    for key in keys {
        let open_start = Instant::now();
        if let Err(err) = engine.open_run(&key) {
            let _ = writeln!(summary, "{key}: open failed: {err}");
            continue;
        }
        let open_ms = open_start.elapsed().as_secs_f64() * 1e3;
        let frame_start = Instant::now();
        let frame = engine.left_heavy_frame(&key, 1024, 1);
        let frame_ms = frame_start.elapsed().as_secs_f64() * 1e3;
        let _ = writeln!(
            summary,
            "{key}: open {open_ms:.2} ms, first left_heavy frame {frame_ms:.2} ms ({} B)",
            frame.len()
        );
        rows.push(BenchRow::new(
            "c6.open.v2_open_ms",
            &key,
            "bexq",
            "open_ms",
            open_ms,
            "ms",
            Basis::Measured,
        ));
        rows.push(BenchRow::new(
            "c6.open.v2_first_frame_ms",
            &key,
            "bexq",
            "first_frame_ms",
            frame_ms,
            "ms",
            Basis::Measured,
        ));
    }
    Ok(ReplayReport { rows, summary })
}
