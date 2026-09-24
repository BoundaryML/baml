//! Runtime-only resource probe. Compile artifacts with the same binary before sampling.
use std::{
    io::Write,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder, TelemetryRecording};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde_json::json;
use sys_native::SysOpsExt;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Compile {
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        artifact: PathBuf,
    },
    Run(Run),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Mode {
    Off,
    AutoNoSink,
    Local,
    CloudFast,
    CloudSlow,
}

#[derive(Args)]
struct Run {
    #[arg(long)]
    artifact: PathBuf,
    #[arg(long, value_enum)]
    mode: Mode,
    #[arg(long)]
    n: i64,
    #[arg(long, default_value_t = 10_000)]
    duration_ms: u64,
    #[arg(long, default_value_t = 0)]
    idle_ms: u64,
    #[arg(long, default_value_t = 1)]
    units_per_root: u64,
    #[arg(long, default_value = "roots")]
    work_unit: String,
    #[arg(long)]
    output_dir: Option<PathBuf>,
    #[arg(long)]
    prepare_base_url: Option<String>,
    /// Defaults to available parallelism, matching `runtime_benchmark`'s `Runtime::new`.
    #[arg(long)]
    workers: Option<NonZeroUsize>,
}

fn emit(value: &serde_json::Value) {
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer(&mut stdout, value).expect("write benchmark protocol");
    stdout.write_all(b"\n").expect("write benchmark newline");
    stdout.flush().expect("flush benchmark protocol");
}

fn marker(origin: Instant, phase: &str) -> f64 {
    let seconds = origin.elapsed().as_secs_f64();
    emit(&json!({"event": "phase", "phase": phase, "elapsed_ms": seconds * 1000.0}));
    seconds
}

fn files(path: &Path) -> Result<(u64, u64)> {
    let mut total = (0, 0);
    if !path.exists() {
        return Ok(total);
    }
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        let next = if metadata.is_dir() {
            files(&entry.path())?
        } else {
            (1, metadata.len())
        };
        total.0 += next.0;
        total.1 += next.1;
    }
    Ok(total)
}

fn run(args: Run) -> Result<()> {
    ensure!(!cfg!(debug_assertions), "run requires a release build");
    let expected = if args.mode == Mode::Off {
        "off"
    } else {
        "medium"
    };
    ensure!(
        std::env::var("BAML_TELEMETRY").as_deref() == Ok(expected),
        "parent must set BAML_TELEMETRY={expected}"
    );
    let origin = Instant::now();
    let load = marker(origin, "load");
    let program = borsh::from_slice::<bex_vm_types::Program>(&std::fs::read(&args.artifact)?)
        .context("load Program artifact (must be compiled by this binary)")?;
    let setup = marker(origin, "setup");
    let workers = args
        .workers
        .unwrap_or(std::thread::available_parallelism()?);
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers.get())
        .enable_all()
        .build()?;
    let config = btel_recorder::RecordingConfig {
        target_bytes: NonZeroUsize::new(1024 * 1024).unwrap(),
        flush_interval_duration: Duration::from_secs(1),
    };
    let recording = match args.mode {
        Mode::Off | Mode::AutoNoSink => None,
        Mode::Local => {
            let directory = args.output_dir.as_ref().context("--output-dir required")?;
            ensure!(!directory.exists(), "output directory must be fresh");
            std::fs::create_dir_all(directory)?;
            let directory = directory.canonicalize()?;
            ensure!(
                directory
                    .ancestors()
                    .any(|p| p.file_name().is_some_and(|n| n == "target")),
                "local output must be inside target"
            );
            Some(TelemetryRecording::local_files_in(directory, config))
        }
        Mode::CloudFast | Mode::CloudSlow => Some(TelemetryRecording::cloud(
            config,
            btel_bcs::CloudPublisherConfig::default(),
            btel_bcs::delivery::DeliveryConfig {
                allow_http: true,
                ..btel_bcs::delivery::DeliveryConfig::new(
                    args.prepare_base_url
                        .context("--prepare-base-url required")?
                        .parse()?,
                )
            },
        )),
    };
    let engine = {
        let _guard = rt.enter();
        Arc::new(match recording {
            Some(recording) => BexEngine::new_with_telemetry_recording(
                program,
                Arc::new(sys_native::SysOps::native()),
                vec![],
                None,
                btel_settings::clock::DEFAULT_MODE,
                recording,
            )?,
            None => BexEngine::new(program, Arc::new(sys_native::SysOps::native()), vec![])?,
        })
    };
    let execute = marker(origin, "execute");
    let initialized = Instant::now();
    let mut invocations = 0u64;
    let mut gc_seconds = 0.0;
    let mut execution_error = None;
    loop {
        let result = rt.block_on(engine.call_function(
            "main",
            vec![BexExternalValue::Int(args.n)],
            FunctionCallContextBuilder::new(sys_native::CallId::next()).build(),
            true,
        ));
        match result {
            Ok(BexExternalValue::Int(value)) if value == args.n => {}
            other => {
                execution_error = Some(format!(
                    "expected main({}) to return its argument: {other:?}",
                    args.n
                ));
                break;
            }
        }
        invocations += 1;
        if invocations.is_multiple_of(8) {
            let gc = Instant::now();
            rt.block_on(engine.collect_garbage(bex_heap::CollectionLevel::Major));
            gc_seconds += gc.elapsed().as_secs_f64();
        }
        if args.idle_ms != 0 {
            std::thread::sleep(Duration::from_millis(args.idle_ms));
        }
        if initialized.elapsed() >= Duration::from_millis(args.duration_ms) {
            break;
        }
    }
    let drain = marker(origin, "drain");
    let shutdown_started = Instant::now();
    rt.block_on(engine.shutdown());
    let drain_ms = shutdown_started.elapsed().as_secs_f64() * 1000.0;
    marker(origin, "done");
    let telemetry = engine
        .telemetry_result()
        .map(|r| r.map_err(|e| e.to_string()));
    let failed = execution_error.is_some() || telemetry.as_ref().is_some_and(Result::is_err);
    let telemetry_status = match &telemetry {
        Some(Err(error)) => error.as_str(),
        _ if args.mode == Mode::Off => "off",
        _ if args.mode == Mode::AutoNoSink => "no-sink",
        _ => "ok",
    };
    let (all, cas) = if let Some(directory) = &args.output_dir {
        (files(directory)?, files(&directory.join("cas"))?)
    } else {
        ((0, 0), (0, 0))
    };
    let work_units = invocations
        .checked_mul(args.units_per_root)
        .context("work unit count overflow")?;
    #[expect(
        clippy::cast_precision_loss,
        reason = "approximate rate; exact count also reported"
    )]
    let throughput_per_s = work_units as f64 / (drain - execute);
    emit(&json!({
        "event": "result", "status": if failed { "error" } else { "ok" },
        "execution_error": execution_error, "telemetry_result": telemetry, "telemetry_status": telemetry_status,
        "load_ms": (setup-load)*1000.0, "setup_ms": (execute-setup)*1000.0,
        "execute_ms": (drain-execute)*1000.0, "drain_ms": drain_ms,
        "invocations": invocations, "work_units": work_units,
        "throughput_per_s": throughput_per_s,
        "work_unit": args.work_unit, "gc_ms": gc_seconds*1000.0,
        "gc_count": invocations / 8, "workers": workers.get(),
        "recording_files": all.0-cas.0, "recording_bytes": all.1-cas.1,
        "cas_files": cas.0, "cas_bytes": cas.1,
        "recording_target_bytes": config.target_bytes.get(),
        "recording_flush_ms": config.flush_interval_duration.as_millis(), "fsync": false,
    }));
    ensure!(!failed, "benchmark execution or telemetry failed");
    Ok(())
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Compile { source, artifact } => {
            let program = baml_db::testing::compile_source(&std::fs::read_to_string(source)?);
            std::fs::write(artifact, borsh::to_vec(&program)?)?;
            Ok(())
        }
        Command::Run(args) => run(args),
    }
}
