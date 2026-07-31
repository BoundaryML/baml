//! Stamped BAML language product version.

pub const CANONICAL_VERSION: &str = "0.15.0";
#[allow(dead_code)]
const PYPI_VERSION: &str = "0.15.0";
pub const CHANNEL: &str = "canary";
#[allow(dead_code)]
const STABLE_VERSION: &str = "0.15.0";

/// Stable compiler identity folded into observability revision ids.
///
/// Release builds may provide `BAML_BUILD_GIT_COMMIT` at compile time. Local
/// builds deliberately use a visible `dev` suffix rather than pretending to
/// be a released compiler. The source snapshot remains a separate hash, so
/// this string only identifies the toolchain half of `source × toolchain ×
/// options`.
#[must_use]
pub fn compiler_id() -> String {
    static ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ID.get_or_init(|| {
        if let Some(commit) = option_env!("BAML_BUILD_GIT_COMMIT").filter(|value| !value.is_empty())
        {
            return format!("{CANONICAL_VERSION}+{CHANNEL}.{commit}");
        }
        #[cfg(not(target_arch = "wasm32"))]
        let executable_hash = std::env::current_exe().and_then(std::fs::read).map_or_else(
            |_| "unavailable".to_owned(),
            |bytes| blake3::hash(&bytes).to_hex().to_string(),
        );
        #[cfg(target_arch = "wasm32")]
        let executable_hash = "wasm".to_owned();
        format!("{CANONICAL_VERSION}+{CHANNEL}.dev+{executable_hash}")
    })
    .clone()
}

#[cfg(test)]
mod tests {
    #[test]
    fn compiler_id_is_nonempty_and_names_version_and_channel() {
        let id = super::compiler_id();
        assert!(id.contains(super::CANONICAL_VERSION));
        assert!(id.contains(super::CHANNEL));
        if option_env!("BAML_BUILD_GIT_COMMIT").is_none() {
            assert!(id.contains(".dev+"));
        }
    }
}
