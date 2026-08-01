use std::{
    fs::{self, OpenOptions},
    io::{BufWriter, Read, Write},
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::{dataset, machine::MachineManifest};

const MAX_TIMEOUT_SECONDS: u64 = 60 * 60;
const MAX_OUTPUT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_REPEATS: u16 = 20;

#[derive(Clone, Copy, Debug)]
pub(crate) enum Pipeline {
    Legacy,
    Dual,
    Cct,
}

impl Pipeline {
    fn as_str(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Dual => "dual",
            Self::Cct => "cct",
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct RunSummary {
    schema_version: u32,
    output: String,
    repeats: u16,
    elapsed_ms: u128,
    bytes: u64,
    pipeline: &'static str,
}

pub(crate) fn run(
    command: &[String],
    output: &Path,
    timeout_seconds: u64,
    max_output_bytes: u64,
    repeat: u16,
    pipeline: Pipeline,
) -> Result<RunSummary> {
    if command.is_empty() {
        bail!("run requires a command after `--`");
    }
    if timeout_seconds == 0 || timeout_seconds > MAX_TIMEOUT_SECONDS {
        bail!("timeout_seconds must be in 1..={MAX_TIMEOUT_SECONDS}");
    }
    if max_output_bytes == 0 || max_output_bytes > MAX_OUTPUT_BYTES {
        bail!("max_output_bytes must be in 1..={MAX_OUTPUT_BYTES}");
    }
    if repeat == 0 || repeat > MAX_REPEATS {
        bail!("repeat must be in 1..={MAX_REPEATS}");
    }
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let manifest = serde_json::to_vec(&serde_json::json!({
        "type": "machine",
        "schema_version": 1,
        "manifest": MachineManifest::collect(),
    }))?;
    let manifest_bytes = u64::try_from(manifest.len())
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    if manifest_bytes > max_output_bytes {
        bail!("machine manifest exceeds bounded output limit {max_output_bytes} bytes");
    }
    let mut manifest_file = BufWriter::new(
        fs::File::create(output).with_context(|| format!("create {}", output.display()))?,
    );
    manifest_file.write_all(&manifest)?;
    manifest_file.write_all(b"\n")?;
    manifest_file.flush()?;
    drop(manifest_file);

    let started = Instant::now();
    for iteration in 0..repeat {
        let stdout = OpenOptions::new()
            .append(true)
            .open(output)
            .with_context(|| format!("open {}", output.display()))?;
        let mut child = Command::new(&command[0])
            .args(&command[1..])
            .env("BAML_PROFILE_PIPELINE", pipeline.as_str())
            .env("BAML_OBS_BENCH_ITERATION", iteration.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("spawn benchmark command `{}`", command.join(" ")))?;
        let child_stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("benchmark child stdout pipe missing"))?;
        let bytes_written = Arc::new(AtomicU64::new(fs::metadata(output)?.len()));
        let exceeded = Arc::new(AtomicBool::new(false));
        let writer_bytes = Arc::clone(&bytes_written);
        let writer_exceeded = Arc::clone(&exceeded);
        let output_thread = thread::spawn(move || -> std::io::Result<()> {
            let mut input = child_stdout;
            let mut output = BufWriter::new(stdout);
            let mut buffer = [0_u8; 8192];
            loop {
                let count = input.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                let next = writer_bytes
                    .load(Ordering::Relaxed)
                    .saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
                if next > max_output_bytes {
                    writer_exceeded.store(true, Ordering::Relaxed);
                    break;
                }
                output.write_all(&buffer[..count])?;
                writer_bytes.store(next, Ordering::Relaxed);
            }
            output.flush()
        });
        let deadline = Instant::now() + Duration::from_secs(timeout_seconds);
        let mut timed_out = false;
        let status = loop {
            if exceeded.load(Ordering::Relaxed) {
                child.kill().ok();
                break child.wait()?;
            }
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() >= deadline {
                child.kill().ok();
                timed_out = true;
                break child.wait()?;
            }
            thread::sleep(Duration::from_millis(20));
        };
        output_thread
            .join()
            .map_err(|_| anyhow::anyhow!("benchmark output thread panicked"))??;
        if exceeded.load(Ordering::Relaxed) {
            bail!("benchmark output exceeded bounded limit {max_output_bytes} bytes");
        }
        if timed_out {
            bail!("benchmark command exceeded {timeout_seconds}s timeout");
        }
        if !status.success() {
            bail!("benchmark command exited with {status}");
        }
    }
    let bytes = fs::metadata(output)?.len();
    if bytes > max_output_bytes {
        bail!("benchmark output exceeded bounded limit {max_output_bytes} bytes");
    }
    dataset::validate(&[output.to_path_buf()])?;
    Ok(RunSummary {
        schema_version: 1,
        output: output.display().to_string(),
        repeats: repeat,
        elapsed_ms: started.elapsed().as_millis(),
        bytes,
        pipeline: pipeline.as_str(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn captures_and_validates_a_bounded_child_row() {
        let output =
            std::env::temp_dir().join(format!("obs-bench-runner-{}.ndjson", std::process::id()));
        let command = vec![
            "sh".to_owned(),
            "-c".to_owned(),
            "printf '%s\\n' '{\"schema_version\":1,\"bench_id\":\"smoke\",\"evidence\":\"measured\",\"value\":1}'"
                .to_owned(),
        ];
        let summary = run(&command, &output, 5, 1024 * 1024, 1, Pipeline::Cct).unwrap();
        assert_eq!(summary.repeats, 1);
        fs::remove_file(output).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn caps_a_runaway_child_before_writing_past_the_limit() {
        let output =
            std::env::temp_dir().join(format!("obs-bench-capped-{}.ndjson", std::process::id()));
        let command = vec![
            "sh".to_owned(),
            "-c".to_owned(),
            "head -c 65536 /dev/zero".to_owned(),
        ];
        let error = run(&command, &output, 5, 16 * 1024, 1, Pipeline::Cct).unwrap_err();
        assert!(error.to_string().contains("bounded limit"));
        assert!(fs::metadata(&output).unwrap().len() <= 16 * 1024);
        fs::remove_file(output).unwrap();
    }
}
