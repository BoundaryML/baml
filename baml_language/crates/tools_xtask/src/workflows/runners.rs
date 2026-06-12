//! `runs-on` runner label constants.
//!
//! Each is a `&'static str` consumed by `Job::runs_on(impl Into<RunsOn>)`.
//! In the gh-workflow fork, `RunsOn: From<T> where T: Into<serde_json::Value>`,
//! so `&str` works directly.

pub const BLACKSMITH_4VCPU: &str = "blacksmith-4vcpu-ubuntu-2404";
pub const BLACKSMITH_8VCPU: &str = "blacksmith-8vcpu-ubuntu-2404";
pub const BLACKSMITH_16VCPU: &str = "blacksmith-16vcpu-ubuntu-2404";
pub const BLACKSMITH_8VCPU_ARM: &str = "blacksmith-8vcpu-ubuntu-2204-arm";
pub const BLACKSMITH_8VCPU_WINDOWS: &str = "blacksmith-8vcpu-windows-2025";
pub const BLACKSMITH_6VCPU_MACOS: &str = "blacksmith-6vcpu-macos-latest";
pub const UBUNTU_LATEST: &str = "ubuntu-latest";
pub const CODSPEED_MACRO: &str = "codspeed-macro";
