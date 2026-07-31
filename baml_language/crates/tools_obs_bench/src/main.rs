//! `obs-bench` — the observability benchmark & acceptance harness
//! (TASK/design.md §10.3). Subcommand map:
//!
//! | subcommand | status | claim families |
//! |---|---|---|
//! | `run` | working | C1 (paired ns/call), C2 (consumer CPU), C3 (bytes) |
//! | `calibrate` | working | C4 (fixed-wall rate sweep legs) |
//! | `prof-stats` | working | context rows; the §10.3 investigation tool |
//! | `value-stats` | working | C5 evidence (dedupe potential) |
//! | `replay` | working | C6 (open/reconstruct path) |
//! | `gen-paths` | working | C4 (distinct-context sweep sources) |
//! | `check` / `refresh-baseline` | working | gate plumbing (size-gate clone) |
//! | `report` | working | the claim ledger |
//! | `crashfuzz` | lands with P3 | C8 |
//! | `validate` | lands with P3 | C8 (artifact validation) |
//! | `corpus` | lands with P3/P5 | C7 (10 GiB synthetic corpus) |
//!
//! Unimplemented subcommands fail loudly with the phase that delivers them —
//! bounded never means silent, including for the tooling itself.

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "CLI harness: stdout is the row/report surface, stderr the progress log"
)]

use std::path::PathBuf;

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use tools_obs_bench::{
    baseline, calibrate, corpus, crashfuzz, gen_paths, machine, prof_stats, replay, report, runner,
    validate, value_stats,
};

#[derive(Parser)]
#[command(
    name = "obs-bench",
    version,
    about = "BAML observability benchmark harness"
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run one workload under a pipeline mode; emit NDJSON measurement rows.
    Run {
        /// Workload .baml file (see crates/tools_obs_bench/workloads/).
        #[arg(long)]
        workload: PathBuf,
        /// Arguments for the workload's generated CLI (repeatable), e.g.
        /// `--args=--iters --args=2500000`.
        #[arg(long = "args", allow_hyphen_values = true)]
        args: Vec<String>,
        /// BAML_PROFILE_PIPELINE for the child. Post-P9 only `cct` exists;
        /// the retired `legacy`/`dual` spellings coerce to `cct` in the
        /// child, so rows they label no longer measure a distinct pipeline.
        #[arg(long, default_value = "cct")]
        pipeline: String,
        /// Also run an unprofiled leg and emit paired c1.* delta rows.
        #[arg(long)]
        paired: bool,
        /// Profiled calls the workload makes (enables per-call rows).
        #[arg(long)]
        calls: Option<u64>,
        /// Append rows to this NDJSON file (default: stdout).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Keep scratch artifacts (profiles, stats) for inspection.
        #[arg(long)]
        keep: bool,
        /// baml-cli binary (default: target/{release,debug}/baml-cli).
        #[arg(long)]
        baml_cli: Option<PathBuf>,
        /// Scratch directory (default: a fresh temp dir).
        #[arg(long)]
        scratch: Option<PathBuf>,
    },
    /// Compare NDJSON rows against the platform baseline; exit 1 on any
    /// violation.
    Check {
        /// NDJSON row files.
        #[arg(required = true)]
        rows: Vec<PathBuf>,
        #[arg(long, default_value = baseline::DEFAULT_BASELINE_DIR)]
        baseline_dir: PathBuf,
    },
    /// Record measured release rows as the new platform baseline.
    RefreshBaseline {
        #[arg(required = true)]
        rows: Vec<PathBuf>,
        #[arg(long, default_value = baseline::DEFAULT_BASELINE_DIR)]
        baseline_dir: PathBuf,
    },
    /// Calibrate fixed-wall iters for the bench_rate work sweep.
    Calibrate {
        #[arg(long)]
        workload: PathBuf,
        #[arg(long, default_value_t = 3.0)]
        target_wall_s: f64,
        /// Work-knob values, comma separated.
        #[arg(long, default_value = "0,10,100,1000")]
        work: String,
        #[arg(long)]
        baml_cli: Option<PathBuf>,
    },
    /// Per-function aggregates of .bamlprof files (files or directories).
    ProfStats {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Also append context rows to this NDJSON file.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Workload label for emitted rows.
        #[arg(long, default_value = "unknown")]
        workload: String,
    },
    /// Capture/byte summary of .bamlvalue files (dedupe potential).
    ValueStats {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long, default_value = "unknown")]
        workload: String,
    },
    /// Time the artifact open/reconstruct path (C6). Default: legacy
    /// `.bamlprof` reconstruct; `--v2-root <.baml>` times the bex_query
    /// open→first-frame path instead.
    Replay {
        #[arg(required_unless_present = "v2_root")]
        paths: Vec<PathBuf>,
        /// A v2 `.baml` root: measure ObserveEngine open + first frame.
        #[arg(long)]
        v2_root: Option<PathBuf>,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Generate distinct-context sweep workloads (C4).
    GenPaths {
        /// Context counts to generate (repeatable).
        #[arg(long = "contexts", required = true)]
        contexts: Vec<u32>,
        #[arg(long, default_value_t = 1_000_000)]
        total_calls: u64,
        /// Output directory.
        #[arg(long)]
        out: PathBuf,
    },
    /// Render the claim ledger from NDJSON rows.
    Report {
        #[arg(required = true)]
        rows: Vec<PathBuf>,
    },
    /// Crash-recovery fuzz (C8): SIGKILL workloads mid-run, then assert
    /// every surviving v2 artifact recovers to its committed prefix.
    Crashfuzz {
        /// Workload `.baml` file to run and kill.
        workload: PathBuf,
        /// Args passed through to the workload after `--`.
        #[arg(last = true)]
        args: Vec<String>,
        /// Kill iterations (delays sweep min..max deterministically).
        #[arg(long, default_value_t = 8)]
        iters: u32,
        #[arg(long, default_value_t = 20)]
        min_delay_ms: u64,
        #[arg(long, default_value_t = 1500)]
        max_delay_ms: u64,
        /// Jitter seed (same seed ⇒ same kill schedule).
        #[arg(long, default_value_t = 0x0b5e_c0de)]
        seed: u64,
        /// `BAML_PROFILE_PIPELINE` for the child (post-P9 only `cct`
        /// exists; retired spellings coerce).
        #[arg(long, default_value = "cct")]
        pipeline: String,
        /// baml-cli binary (defaults to the workspace release build).
        #[arg(long)]
        baml_cli: Option<PathBuf>,
        /// Scratch dir (default: a temp dir).
        #[arg(long)]
        scratch: Option<PathBuf>,
    },
    /// Validate on-disk observability artifacts under a `.baml`-shaped
    /// root: framing, CRCs, committed-prefix recovery (C8).
    Validate {
        /// The root holding `sessions/`, `history/`, `dict/`, `profiles/`.
        root: PathBuf,
        /// Emit the full JSON report instead of the human summary.
        #[arg(long)]
        json: bool,
    },
    /// Seeded synthetic sealed-segment corpus (C7): generate + scan gate.
    Corpus {
        #[command(subcommand)]
        action: CorpusAction,
    },
}

#[derive(Subcommand)]
enum CorpusAction {
    /// Generate ~total-bytes of sealed segments under <root>/sessions/.
    Synth {
        /// Corpus root (a `.baml`-shaped dir; created if missing).
        root: PathBuf,
        #[arg(long, default_value_t = 100 << 20)]
        total_bytes: u64,
        #[arg(long, default_value_t = 8 << 20)]
        session_bytes: u64,
        #[arg(long, default_value_t = 2000)]
        nodes_per_session: u32,
        #[arg(long, default_value_t = 0xC0DE)]
        seed: u64,
    },
    /// Fold every session; emit c7.* rows (wall, MB/s, peak RSS).
    Scan {
        root: PathBuf,
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Run {
            workload,
            args,
            pipeline,
            paired,
            calls,
            out,
            keep,
            baml_cli,
            scratch,
        } => {
            let baml_cli = match baml_cli {
                Some(p) => p,
                None => runner::default_baml_cli()?,
            };
            let scratch = scratch.unwrap_or_else(|| {
                std::env::temp_dir().join(format!("obs-bench-{}", std::process::id()))
            });
            let report = runner::run(&runner::RunConfig {
                workload,
                args,
                pipeline,
                baml_cli,
                scratch,
                paired,
                calls,
                keep,
            })?;
            eprint!("{}", report.summary);
            emit_rows(&report.rows, out.as_deref())?;
        }
        Cmd::Check { rows, baseline_dir } => {
            let rows = report::load_rows(&rows)?;
            let platform = machine::platform_key();
            let Some(baseline) = baseline::load(&baseline_dir, &platform)? else {
                eprintln!(
                    "obs-bench: no baseline for {platform} under {}; run refresh-baseline first",
                    baseline_dir.display()
                );
                std::process::exit(2);
            };
            let (checked, violations, skipped) = baseline::check(&baseline, &rows);
            for s in &skipped {
                eprintln!("obs-bench: skipped: {s}");
            }
            println!("checked {checked} gated rows against {platform} baseline");
            if !violations.is_empty() {
                for v in &violations {
                    println!("VIOLATION {}: {}", v.bench_id, v.message);
                }
                std::process::exit(1);
            }
        }
        Cmd::RefreshBaseline { rows, baseline_dir } => {
            let rows = report::load_rows(&rows)?;
            let platform = machine::platform_key();
            let path = baseline::refresh(&baseline_dir, &platform, &rows)?;
            println!("baseline written: {}", path.display());
        }
        Cmd::Calibrate {
            workload,
            target_wall_s,
            work,
            baml_cli,
        } => {
            let work_values: Vec<u64> = work
                .split(',')
                .map(|s| s.trim().parse().context("parsing --work"))
                .collect::<Result<_, _>>()?;
            let legs = calibrate::run(&calibrate::CalibrateConfig {
                workload,
                target_wall_s,
                work_values,
                scratch: std::env::temp_dir().join(format!("obs-cal-{}", std::process::id())),
                baml_cli,
            })?;
            for leg in legs {
                println!("{}", serde_json::to_string(&leg)?);
            }
        }
        Cmd::ProfStats {
            paths,
            out,
            workload,
        } => {
            let stats = prof_stats::run(&paths)?;
            print!("{}", stats.render_table());
            emit_rows(&stats.to_rows(&workload, "legacy"), out.as_deref())?;
        }
        Cmd::ValueStats {
            paths,
            out,
            workload,
        } => {
            let stats = value_stats::run(&paths)?;
            print!("{}", stats.render_table());
            emit_rows(&stats.to_rows(&workload, "legacy"), out.as_deref())?;
        }
        Cmd::Replay {
            paths,
            v2_root,
            out,
        } => {
            let rep = match v2_root {
                Some(root) => replay::run_v2(&root)?,
                None => replay::run(&paths, "legacy")?,
            };
            eprint!("{}", rep.summary);
            emit_rows(&rep.rows, out.as_deref())?;
        }
        Cmd::GenPaths {
            contexts,
            total_calls,
            out,
        } => {
            std::fs::create_dir_all(&out).with_context(|| format!("creating {}", out.display()))?;
            for p in contexts {
                let (source, reps) = gen_paths::generate(p, total_calls);
                let path = out.join(gen_paths::file_name(p));
                std::fs::write(&path, source)
                    .with_context(|| format!("writing {}", path.display()))?;
                println!(
                    "{}: {} contexts, run with `-- --reps {reps}` for {} calls",
                    path.display(),
                    p,
                    reps * u64::from(p)
                );
            }
        }
        Cmd::Report { rows } => {
            let rows = report::load_rows(&rows)?;
            print!("{}", report::render(&rows));
        }
        Cmd::Crashfuzz {
            workload,
            args,
            iters,
            min_delay_ms,
            max_delay_ms,
            seed,
            pipeline,
            baml_cli,
            scratch,
        } => {
            let baml_cli = match baml_cli {
                Some(p) => p,
                None => runner::default_baml_cli()?,
            };
            let scratch = scratch.unwrap_or_else(|| {
                std::env::temp_dir().join(format!("obs-crashfuzz-{}", std::process::id()))
            });
            let report = crashfuzz::crashfuzz(&crashfuzz::FuzzConfig {
                workload,
                args,
                baml_cli,
                scratch,
                pipeline,
                iters,
                min_delay_ms,
                max_delay_ms,
                seed,
            })?;
            eprint!("{}", report.summary);
            anyhow::ensure!(
                report.failures.is_empty(),
                "crashfuzz failures:\n{}",
                report.failures.join("\n")
            );
        }
        Cmd::Validate { root, json } => {
            let report = validate::validate_root(&root);
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", report.render());
            }
            anyhow::ensure!(
                report.invalid == 0,
                "{} invalid artifact(s) under {}",
                report.invalid,
                root.display()
            );
        }
        Cmd::Corpus { action } => match action {
            CorpusAction::Synth {
                root,
                total_bytes,
                session_bytes,
                nodes_per_session,
                seed,
            } => {
                let written =
                    corpus::synth(&root, total_bytes, session_bytes, nodes_per_session, seed)?;
                eprintln!("corpus: wrote {written} bytes under {}", root.display());
            }
            CorpusAction::Scan { root, out } => {
                let rep = corpus::scan(&root)?;
                eprint!("{}", rep.summary);
                emit_rows(&rep.rows, out.as_deref())?;
            }
        },
    }
    Ok(())
}

fn emit_rows(
    rows: &[tools_obs_bench::rows::BenchRow],
    out: Option<&std::path::Path>,
) -> anyhow::Result<()> {
    match out {
        Some(path) => {
            use std::io::Write as _;
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .with_context(|| format!("opening {}", path.display()))?;
            for row in rows {
                writeln!(file, "{}", row.to_ndjson())?;
            }
            eprintln!(
                "obs-bench: {} rows appended to {}",
                rows.len(),
                path.display()
            );
        }
        None => {
            for row in rows {
                println!("{}", row.to_ndjson());
            }
        }
    }
    Ok(())
}
