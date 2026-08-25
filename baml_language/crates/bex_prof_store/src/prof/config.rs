//! Process profiling switch.
//!
//! Resource sizing belongs exclusively to `ProfilerConfig` and
//! `ProfilerSizingPolicy`; the transport has no independent production
//! capacity knobs.

use std::sync::OnceLock;

/// Master profiling switch. Unset is enabled on native and disabled on WASM.
pub const ENV_PROFILE: &str = "BAML_PROFILE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfConfig {
    pub enabled: bool,
}

// On wasm32 the default is `false`, which clippy flags as derivable; the
// target-dependent literal is the point.
#[cfg_attr(target_arch = "wasm32", allow(clippy::derivable_impls))]
impl Default for ProfConfig {
    fn default() -> Self {
        Self {
            enabled: cfg!(not(target_arch = "wasm32")),
        }
    }
}

impl ProfConfig {
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    #[must_use]
    pub fn global() -> &'static Self {
        static GLOBAL: OnceLock<ProfConfig> = OnceLock::new();
        GLOBAL.get_or_init(Self::from_env)
    }

    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Self {
        let enabled = get(ENV_PROFILE)
            .map(|value| {
                let value = value.trim();
                value == "1" || value.eq_ignore_ascii_case("true")
            })
            .unwrap_or(cfg!(not(target_arch = "wasm32")));
        Self {
            enabled: enabled && cfg!(not(target_arch = "wasm32")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| {
            pairs
                .iter()
                .find(|(candidate, _)| *candidate == key)
                .map(|(_, value)| (*value).to_string())
        }
    }

    #[test]
    fn defaults_when_unset() {
        assert_eq!(ProfConfig::from_lookup(lookup(&[])), ProfConfig::default());
    }

    #[test]
    fn truthiness_is_closed() {
        for value in ["1", "true", "TRUE", " true "] {
            assert_eq!(
                ProfConfig::from_lookup(lookup(&[(ENV_PROFILE, value)])).enabled,
                cfg!(not(target_arch = "wasm32"))
            );
        }
        for value in ["0", "false", "", "yes", "2", "on"] {
            assert!(!ProfConfig::from_lookup(lookup(&[(ENV_PROFILE, value)])).enabled);
        }
    }
}
