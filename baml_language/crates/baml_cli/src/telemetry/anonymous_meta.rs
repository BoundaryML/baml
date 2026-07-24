//! Memoized system-level metadata attached to every telemetry event. Direct
//! port of Next.js's `packages/next/src/telemetry/anonymous-meta.ts`.
//!
//! Everything here is coarse and non-identifying: OS name, CPU arch, CPU
//! count, whether we're in CI/Docker/WSL, CLI version and channel. See
//! `TELEMETRY.md` for the full field-by-field list.

use std::sync::OnceLock;

use serde::Serialize;

use super::storage::env_is_truthy;

/// The set of system-scoped fields we attach to every telemetry event.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct AnonymousMeta {
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
    /// Compile-time CLI version constant, e.g. `0.14.1`.
    pub cli_version: &'static str,
    /// Compile-time release channel: `stable`, `canary`, `dev`.
    pub channel: &'static str,
}

/// Memoized accessor. Computed once per process; the returned reference
/// is `'static`. Matches Next's module-level `traits` cache.
pub(crate) fn get() -> &'static AnonymousMeta {
    static META: OnceLock<AnonymousMeta> = OnceLock::new();
    META.get_or_init(compute)
}

fn compute() -> AnonymousMeta {
    let cpu_count = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(0);

    let ci_name = std::env::var("CI").ok().filter(|v| !v.is_empty());

    AnonymousMeta {
        system_platform: std::env::consts::OS,
        system_architecture: std::env::consts::ARCH,
        cpu_count,
        is_docker: detect_docker(),
        is_wsl: detect_wsl(),
        is_ci: env_is_truthy("CI"),
        ci_name,
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

    /// The memoized `AnonymousMeta` reports something plausible for the
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
