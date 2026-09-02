//! Memoized system and environment metadata attached to every telemetry event.
//! Originally ported from Next.js's `packages/next/src/telemetry/anonymous-meta.ts`.
//!
//! See `TELEMETRY.md` for the full field-by-field list and data-use policy.

use std::sync::OnceLock;

use serde::Serialize;

use super::storage::env_is_truthy;

/// The set of system-scoped fields we attach to every telemetry event.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct EnvironmentMeta {
    /// `macos`, `linux`, `windows`, … (Rust's `std::env::consts::OS`).
    pub system_platform: &'static str,
    /// Build-target architecture: `aarch64`, `x86_64`, … (`std::env::consts::ARCH`).
    pub system_architecture: &'static str,
    /// Logical CPU count as reported by `std::thread::available_parallelism`.
    /// Falls back to 0 on the (unusual) platforms where std can't tell.
    pub cpu_count: usize,
    /// `true` if `/.dockerenv` or `/proc/1/cgroup` indicates we're inside a
    /// Docker container. Best-effort; false on non-Linux.
    pub is_docker: bool,
    /// `true` if `/proc/version` mentions WSL / Microsoft. False on non-Linux.
    pub is_wsl: bool,
    /// `true` if `$CI` is set to a truthy value.
    pub is_ci: bool,
    /// Raw value of `$CI` when set (usually `true` or a provider name like
    /// `GitHub Actions`). Left as-is; not interpreted.
    pub ci_name: Option<String>,
    /// Detected coding-agent harness, or `None` for an apparently human run.
    pub agent_harness: Option<String>,
    /// Machine hostname, when the operating system exposes one.
    pub machine_hostname: Option<String>,
    /// Raw value of `$HOME`, when set.
    pub home: Option<String>,
    /// Compile-time CLI version constant, e.g. `0.14.1`.
    pub cli_version: &'static str,
    /// Compile-time release channel: `stable`, `canary`, `dev`.
    pub channel: &'static str,
}

/// Memoized accessor. Computed once per process; the returned reference
/// is `'static`. Matches Next's module-level `traits` cache.
pub(crate) fn get() -> &'static EnvironmentMeta {
    static META: OnceLock<EnvironmentMeta> = OnceLock::new();
    META.get_or_init(compute)
}

fn compute() -> EnvironmentMeta {
    let cpu_count = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(0);

    let ci_name = std::env::var("CI").ok().filter(|v| !v.is_empty());

    EnvironmentMeta {
        system_platform: std::env::consts::OS,
        system_architecture: std::env::consts::ARCH,
        cpu_count,
        is_docker: detect_docker(),
        is_wsl: detect_wsl(),
        is_ci: env_is_truthy("CI"),
        ci_name,
        agent_harness: crate::agent_harness::detect(),
        machine_hostname: hostname::get()
            .ok()
            .and_then(|hostname| hostname.into_string().ok())
            .filter(|hostname| !hostname.is_empty()),
        home: std::env::var("HOME").ok().filter(|home| !home.is_empty()),
        cli_version: baml_version::CANONICAL_VERSION,
        channel: baml_version::CHANNEL,
    }
}

/// Best-effort Docker detection. On Linux, the two conventional signals are
/// `/.dockerenv` (canonical marker file) and cgroup entries mentioning
/// "docker". On other OSes we return `false`; container platforms there
/// (e.g. macOS's `com.docker.docker`) run Linux inside a VM anyway, so
/// this check would fire from inside the container regardless.
fn detect_docker() -> bool {
    if std::path::Path::new("/.dockerenv").exists() {
        return true;
    }
    if let Ok(cgroup) = std::fs::read_to_string("/proc/1/cgroup") {
        if cgroup.contains("docker") || cgroup.contains("containerd") {
            return true;
        }
    }
    false
}

/// Best-effort WSL detection: `/proc/version` mentions `microsoft` or
/// `WSL` when running under WSL1/WSL2. False on non-Linux.
fn detect_wsl() -> bool {
    if let Ok(version) = std::fs::read_to_string("/proc/version") {
        let v = version.to_ascii_lowercase();
        return v.contains("microsoft") || v.contains("wsl");
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The memoized `EnvironmentMeta` reports something plausible for the
    /// current platform. We can't assert exact values (CI runs on
    /// arbitrary hardware), but we can assert basic invariants.
    #[test]
    fn meta_reports_this_platform() {
        let m = get();
        assert_eq!(m.system_platform, std::env::consts::OS);
        assert_eq!(m.system_architecture, std::env::consts::ARCH);
        assert_eq!(m.cli_version, baml_version::CANONICAL_VERSION);
        assert_eq!(m.channel, baml_version::CHANNEL);
    }

    /// The accessor is memoized: repeated calls return the same reference.
    #[test]
    fn meta_is_memoized() {
        let a = get() as *const _;
        let b = get() as *const _;
        assert_eq!(a, b);
    }
}
