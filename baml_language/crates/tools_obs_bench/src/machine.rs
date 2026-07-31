//! `MachineManifest` — the per-run environment record embedded in every
//! bench row (design §10.2): OS/arch/CPU/governor/clock/runner class.
//! Gates never compare across machines; the manifest is what makes that
//! rule checkable.

use serde::{Deserialize, Serialize};

/// Environment context embedded in every [`crate::rows::BenchRow`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineManifest {
    /// `std::env::consts::OS` (`linux`, `macos`, `windows`).
    pub os: String,
    /// `std::env::consts::ARCH` (`x86_64`, `aarch64`).
    pub arch: String,
    /// Rust target-triple-style platform key used for baseline files,
    /// mirroring the size gate (`x86_64-unknown-linux-gnu`, ...).
    pub platform: String,
    /// CPU model string, best effort (`/proc/cpuinfo` on Linux).
    pub cpu_model: Option<String>,
    /// Logical CPU count.
    pub num_cpus: usize,
    /// Linux cpufreq governor of cpu0, best effort (`performance`,
    /// `schedutil`, ...). Bench legs that need a pinned governor assert on
    /// this field instead of hoping.
    pub governor: Option<String>,
    /// Profiling clock source (`tsc`, `cntvct`, `instant`, `stub`) as
    /// detected by `bex_events::prof::clock`.
    pub clock_kind: String,
    /// Build profile of this harness binary (`release` | `debug`). Rows from
    /// debug builds never gate.
    pub build_profile: String,
    /// Runner classification: `BAML_OBS_RUNNER_CLASS` env (CI sets e.g.
    /// `gha-ubuntu-16core`), defaulting to `local`.
    pub runner_class: String,
}

impl MachineManifest {
    /// Collect the manifest for this process. Cheap (a few file reads);
    /// call once per row batch, not per event.
    pub fn collect() -> MachineManifest {
        MachineManifest {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            platform: platform_key(),
            cpu_model: cpu_model(),
            num_cpus: std::thread::available_parallelism().map_or(0, std::num::NonZero::get),
            governor: governor(),
            clock_kind: clock_kind(),
            build_profile: if cfg!(debug_assertions) {
                "debug".to_string()
            } else {
                "release".to_string()
            },
            runner_class: std::env::var("BAML_OBS_RUNNER_CLASS")
                .unwrap_or_else(|_| "local".to_string()),
        }
    }
}

/// The size-gate-style platform key for baseline file naming.
pub fn platform_key() -> String {
    // Mirror the committed baseline names in .ci/size-gate/.
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu".to_string(),
        ("aarch64", "linux") => "aarch64-unknown-linux-gnu".to_string(),
        ("x86_64", "macos") => "x86_64-apple-darwin".to_string(),
        ("aarch64", "macos") => "aarch64-apple-darwin".to_string(),
        ("x86_64", "windows") => "x86_64-pc-windows-msvc".to_string(),
        (arch, os) => format!("{arch}-{os}"),
    }
}

fn cpu_model() -> Option<String> {
    if cfg!(target_os = "linux") {
        let info = std::fs::read_to_string("/proc/cpuinfo").ok()?;
        for line in info.lines() {
            if let Some(rest) = line.strip_prefix("model name") {
                return Some(rest.trim_start_matches([' ', '\t', ':']).trim().to_string());
            }
        }
        None
    } else {
        None
    }
}

fn governor() -> Option<String> {
    std::fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
        .ok()
        .map(|s| s.trim().to_string())
}

fn clock_kind() -> String {
    use bex_events::prof::clock;
    // `meta()` forces clock detection; fine — this is the bench harness.
    match clock::meta().kind {
        clock::ClockKind::Tsc => "tsc",
        clock::ClockKind::Cntvct => "cntvct",
        clock::ClockKind::Instant => "instant",
        clock::ClockKind::Stub => "stub",
    }
    .to_string()
}

/// Current git commit, best effort (rows carry it so a number can always be
/// traced to a tree).
pub fn git_sha() -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let sha = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!sha.is_empty()).then_some(sha)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_collects_basic_fields() {
        let m = MachineManifest::collect();
        assert!(!m.os.is_empty());
        assert!(!m.arch.is_empty());
        assert!(m.num_cpus > 0);
        assert!(["tsc", "cntvct", "instant", "stub"].contains(&m.clock_kind.as_str()));
        assert!(m.platform.contains('-'));
    }
}
