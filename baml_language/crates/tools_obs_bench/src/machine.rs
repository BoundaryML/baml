use std::{collections::BTreeMap, fs, process::Command};

use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct MachineManifest {
    schema_version: u32,
    os: String,
    arch: String,
    cpu_count: usize,
    cpu_model: Option<String>,
    disk_kind: String,
    governor: Option<String>,
    clock: String,
    rustc: Option<String>,
    git_sha: Option<String>,
    profile_pipeline: Option<String>,
    durability: Option<String>,
    runner_class: Option<String>,
    environment: BTreeMap<String, String>,
}

impl MachineManifest {
    pub(crate) fn collect() -> Self {
        Self {
            schema_version: 1,
            os: std::env::consts::OS.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
            cpu_count: std::thread::available_parallelism().map_or(1, usize::from),
            cpu_model: cpu_model(),
            disk_kind: std::env::var("BAML_BENCH_DISK_KIND")
                .unwrap_or_else(|_| "unknown".to_owned()),
            governor: fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
                .ok()
                .map(|value| value.trim().to_owned()),
            clock: std::env::var("BAML_BENCH_CLOCK").unwrap_or_else(|_| "monotonic".to_owned()),
            rustc: command_line("rustc", &["--version"]),
            git_sha: command_line("git", &["rev-parse", "HEAD"]),
            profile_pipeline: std::env::var("BAML_PROFILE_PIPELINE").ok(),
            durability: std::env::var("BAML_PROFILE_DURABILITY").ok(),
            runner_class: std::env::var("RUNNER_NAME")
                .or_else(|_| std::env::var("BAML_BENCH_RUNNER_CLASS"))
                .ok(),
            environment: ["BAML_PROFILE_RAW", "BAML_PROFILE_OVERFLOW"]
                .into_iter()
                .filter_map(|name| {
                    std::env::var(name)
                        .ok()
                        .map(|value| (name.to_owned(), value))
                })
                .collect(),
        }
    }
}

fn cpu_model() -> Option<String> {
    if let Ok(contents) = fs::read_to_string("/proc/cpuinfo")
        && let Some(value) = contents.lines().find_map(|line| {
            line.split_once(':')
                .filter(|(name, _)| matches!(name.trim(), "model name" | "Hardware"))
                .map(|(_, value)| value.trim().to_owned())
        })
    {
        return Some(value);
    }
    command_line("sysctl", &["-n", "machdep.cpu.brand_string"])
}

fn command_line(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
