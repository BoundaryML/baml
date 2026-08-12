//! Stamped BAML language product version.
//!
//! Managed by `scripts/baml-language-version` (`stamp` rewrites the
//! constant from the frozen release plan at packaging time; `check`
//! enforces that the committed value equals the canary `release.toml`
//! version). Keeping the committed value canonical is load-bearing: the
//! loader's post-load `version()` handshake compares against it, in
//! development builds too.

pub(crate) const BRIDGE_RUNTIME_NAME: &str = "baml_bridge";
pub(crate) const TOOLCHAIN_VERSION: &str = "0.16.0";
pub(crate) const BRIDGE_RUNTIME_VERSION: &str = "0.16.0";
