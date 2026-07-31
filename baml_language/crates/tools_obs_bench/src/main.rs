#![allow(clippy::print_stdout, clippy::print_stderr)]

mod artifact;
mod baseline;
mod corpus;
mod dataset;
mod gen_paths;
mod machine;
mod prof_stats;
mod replay;
mod report;
mod row;
mod runner;
mod value_stats;

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use clap::{Parser, Subcommand, ValueEnum};

const MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

#[derive(Parser)]
#[command(name = "obs-bench", version, about)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a benchmark child with bounded time/output and embed a machine manifest.
    Run {
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 300)]
        timeout_seconds: u64,
        #[arg(long, default_value_t = 64 * 1024 * 1024)]
        max_output_bytes: u64,
        #[arg(long, default_value_t = 1)]
        repeat: u16,
        #[arg(long, value_enum, default_value = "cct")]
        pipeline: PipelineArg,
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Enforce measured rows against the committed per-platform TOML baseline.
    Check {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        #[arg(long)]
        baseline: Option<PathBuf>,
        #[arg(long)]
        platform: Option<String>,
        #[arg(long)]
        allow_unclassified_evidence: bool,
        #[arg(long, value_enum, default_value = "json")]
        format: OutputFormat,
    },
    /// Adopt measured rows into a per-platform TOML baseline.
    RefreshBaseline {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        #[arg(long)]
        baseline: Option<PathBuf>,
        #[arg(long)]
        platform: Option<String>,
        #[arg(long, default_value_t = 15.0)]
        max_delta_pct: f64,
        #[arg(long, value_enum, default_value = "json")]
        format: OutputFormat,
    },
    /// Print the current machine manifest used to qualify measurements.
    Calibrate {
        #[arg(long, value_enum, default_value = "json")]
        format: OutputFormat,
    },
    /// Inspect one or more legacy `.bamlprof` artifacts.
    ProfStats {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        #[arg(long, value_enum, default_value = "json")]
        format: OutputFormat,
    },
    /// Inspect one or more `.bamlvalue` artifacts.
    ValueStats {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        #[arg(long, value_enum, default_value = "json")]
        format: OutputFormat,
    },
    /// Decode an artifact into stable NDJSON records for oracles and diffs.
    Replay { path: PathBuf },
    /// Exercise artifact recovery at deterministic crash/truncation offsets.
    Crashfuzz {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        #[arg(long, default_value_t = 100)]
        iterations: u32,
        #[arg(long, default_value_t = 0xB4_4D_4C)]
        seed: u64,
        #[arg(long, value_enum, default_value = "json")]
        format: OutputFormat,
    },
    /// Validate observability artifacts or benchmark row files.
    Validate {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        #[arg(long, value_enum, default_value = "json")]
        format: OutputFormat,
    },
    /// Scan or synthesize deterministic observability corpora.
    Corpus {
        #[command(subcommand)]
        command: CorpusCommand,
    },
    /// Generate a deterministic P-path BAML workload at constant total calls.
    GenPaths {
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        paths: u32,
        #[arg(long, default_value_t = 1_000_000)]
        total_calls: u64,
        #[arg(long, value_enum, default_value = "json")]
        format: OutputFormat,
    },
    /// Render a claim ledger from benchmark NDJSON/JSON rows.
    Report {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        #[arg(long, value_enum, default_value = "markdown")]
        format: ReportFormat,
    },
}

#[derive(Subcommand)]
enum CorpusCommand {
    Scan {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        #[arg(long, value_enum, default_value = "json")]
        format: OutputFormat,
    },
    Synth {
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 1024 * 1024)]
        target_bytes: u64,
        #[arg(long, default_value_t = 256)]
        nodes: u32,
        #[arg(long, default_value_t = 0x5E_ED)]
        seed: u64,
        #[arg(long, value_enum, default_value = "json")]
        format: OutputFormat,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum PipelineArg {
    Legacy,
    Dual,
    Cct,
}

#[derive(Clone, Copy, ValueEnum)]
enum OutputFormat {
    Json,
    Ndjson,
}

#[derive(Clone, Copy, ValueEnum)]
enum ReportFormat {
    Json,
    Markdown,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("obs-bench: {err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    match Args::parse().command {
        Command::Run {
            output,
            timeout_seconds,
            max_output_bytes,
            repeat,
            pipeline,
            command,
        } => emit_one(
            &runner::run(
                &command,
                &output,
                timeout_seconds,
                max_output_bytes,
                repeat,
                match pipeline {
                    PipelineArg::Legacy => runner::Pipeline::Legacy,
                    PipelineArg::Dual => runner::Pipeline::Dual,
                    PipelineArg::Cct => runner::Pipeline::Cct,
                },
            )?,
            OutputFormat::Json,
        )?,
        Command::Check {
            paths,
            baseline,
            platform,
            allow_unclassified_evidence,
            format,
        } => {
            let platform = platform.unwrap_or_else(dataset::platform_label);
            let baseline = baseline.unwrap_or_else(|| {
                dataset::default_baseline_path(Path::new(MANIFEST_DIR), &platform)
            });
            let report = baseline::check(&paths, &baseline, !allow_unclassified_evidence)?;
            emit_one(&report, format)?;
            if !report.passed {
                bail!("observability baseline check failed");
            }
        }
        Command::RefreshBaseline {
            paths,
            baseline,
            platform,
            max_delta_pct,
            format,
        } => {
            let platform = platform.unwrap_or_else(dataset::platform_label);
            let baseline_path = baseline.unwrap_or_else(|| {
                dataset::default_baseline_path(Path::new(MANIFEST_DIR), &platform)
            });
            let baseline = baseline::refresh(&paths, &baseline_path, platform, max_delta_pct)?;
            emit_one(&baseline, format)?;
        }
        Command::ProfStats { paths, format } => {
            emit(&prof_stats::inspect_all(&paths)?, format)?;
        }
        Command::ValueStats { paths, format } => {
            emit(&value_stats::inspect_all(&paths)?, format)?;
        }
        Command::Replay { path } => replay::replay(&path)?,
        Command::Calibrate { format } => {
            emit_one(&machine::MachineManifest::collect(), format)?;
        }
        Command::Crashfuzz {
            paths,
            iterations,
            seed,
            format,
        } => emit(&artifact::crashfuzz(&paths, iterations, seed)?, format)?,
        Command::Validate { paths, format } => {
            emit(&artifact::validate(&paths)?, format)?;
        }
        Command::Corpus { command } => match command {
            CorpusCommand::Scan { paths, format } => {
                emit_one(&corpus::scan(&paths)?, format)?;
            }
            CorpusCommand::Synth {
                output,
                target_bytes,
                nodes,
                seed,
                format,
            } => emit_one(&corpus::synth(&output, target_bytes, nodes, seed)?, format)?,
        },
        Command::GenPaths {
            output,
            paths,
            total_calls,
            format,
        } => {
            let output =
                output.unwrap_or_else(|| gen_paths::default_output(Path::new(MANIFEST_DIR)));
            emit_one(&gen_paths::generate(&output, paths, total_calls)?, format)?;
        }
        Command::Report { paths, format } => {
            let ledger = report::build(&paths)?;
            match format {
                ReportFormat::Json => emit_one(&ledger, OutputFormat::Json)?,
                ReportFormat::Markdown => println!("{}", report::markdown(&ledger)),
            }
        }
    }
    Ok(())
}

fn emit<T: serde::Serialize>(values: &[T], format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(values)?),
        OutputFormat::Ndjson => {
            for value in values {
                println!("{}", serde_json::to_string(value)?);
            }
        }
    }
    Ok(())
}

fn emit_one<T: serde::Serialize>(value: &T, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(value)?),
        OutputFormat::Ndjson => println!("{}", serde_json::to_string(value)?),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_command_surface_parses() {
        for command in [
            vec![
                "obs-bench",
                "run",
                "--output",
                "rows.ndjson",
                "--",
                "bench-producer",
            ],
            vec!["obs-bench", "calibrate"],
            vec!["obs-bench", "check", "rows.ndjson"],
            vec!["obs-bench", "refresh-baseline", "rows.ndjson"],
            vec!["obs-bench", "prof-stats", "run.bamlprof"],
            vec!["obs-bench", "value-stats", "run.bamlvalue"],
            vec!["obs-bench", "replay", "run.bamlcct"],
            vec!["obs-bench", "crashfuzz", "run.bamlprof"],
            vec!["obs-bench", "validate", "rows.ndjson"],
            vec!["obs-bench", "corpus", "scan", "."],
            vec!["obs-bench", "corpus", "synth", "--output", "corpus"],
            vec![
                "obs-bench",
                "gen-paths",
                "--paths",
                "4",
                "--output",
                "paths.baml",
            ],
            vec!["obs-bench", "report", "rows.ndjson"],
        ] {
            Args::try_parse_from(command).unwrap();
        }
    }
}
