//! Where an engine's heap gets its collection policy.
//!
//! Each setting resolves on its own, highest first: the environment variable,
//! the value a publisher baked into a packed program (`[pack.gc]`), the
//! default. The engine resolves once, when it is built; a bad value is an
//! engine construction error naming its source, never a silent fallback.
//!
//! ```text
//! BAML_GC_GROWTH=2.5          # growth: a multiple of the data that survived
//! BAML_GC_MIN_GROWTH=8MiB     # min_growth: bytes, with B/KiB/MiB/GiB
//! ```
use std::ffi::OsString;

pub use bex_heap::gc_policy::{GcSettingError, GcSettings, Growth, parse_min_growth};

/// Overrides `growth` for source runs, tests, bridges and (unless renamed by
/// the publisher) packed programs.
pub const GROWTH_ENV_VAR: &str = "BAML_GC_GROWTH";
/// Overrides `min_growth`, likewise.
pub const MIN_GROWTH_ENV_VAR: &str = "BAML_GC_MIN_GROWTH";

/// The collection policy an engine is asked to use, before the environment.
///
/// [`Default`] is what every source run, test and bridge uses: nothing baked,
/// the standard variable names. A packed program carries the publisher's
/// values and the names it reads (see `baml_exec::PackGc`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GcConfig {
    /// Baked `growth`; `None` keeps the default.
    pub growth: Option<Growth>,
    /// Baked `min_growth` in bytes; `None` keeps the default. Validated by
    /// [`GcConfig::resolve_with`] with the rest.
    pub min_growth: Option<usize>,
    /// The variable that overrides `growth`.
    pub growth_env_var: String,
    /// The variable that overrides `min_growth`.
    pub min_growth_env_var: String,
}

impl Default for GcConfig {
    fn default() -> Self {
        Self {
            growth: None,
            min_growth: None,
            growth_env_var: GROWTH_ENV_VAR.to_owned(),
            min_growth_env_var: MIN_GROWTH_ENV_VAR.to_owned(),
        }
    }
}

/// A GC setting that cannot work, with where it came from.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GcConfigError {
    #[error("invalid GC setting: {0}")]
    Env(#[from] baml_env::EnvError),
    #[error("invalid GC setting {source_name}: {error}")]
    Value {
        /// The environment variable, or `[pack.gc] <key>` for a baked value.
        source_name: String,
        error: GcSettingError,
    },
}

impl GcConfig {
    /// Resolve against the process environment.
    pub fn resolve(&self) -> Result<GcSettings, GcConfigError> {
        self.resolve_with(baml_env::os_var)
    }

    /// Resolve against an injected environment (tests, hosts). Empty or
    /// whitespace-only values are unset, as for every `BAML_*` variable.
    pub fn resolve_with(
        &self,
        mut read: impl FnMut(&str) -> Option<OsString>,
    ) -> Result<GcSettings, GcConfigError> {
        let defaults = GcSettings::default();
        let mut env = |name: &str| baml_env::string_from(name, read(name));
        let at = |source_name: &str| {
            let source_name = source_name.to_owned();
            move |error| GcConfigError::Value { source_name, error }
        };

        let growth = match env(&self.growth_env_var)? {
            Some(text) => Growth::parse(&text).map_err(at(&self.growth_env_var))?,
            None => self.growth.unwrap_or(defaults.growth()),
        };
        let (min_growth, min_source) = match env(&self.min_growth_env_var)? {
            Some(text) => (
                parse_min_growth(&text).map_err(at(&self.min_growth_env_var))?,
                self.min_growth_env_var.as_str(),
            ),
            None => (
                self.min_growth.unwrap_or(defaults.min_growth()),
                "[pack.gc] min_growth",
            ),
        };
        GcSettings::new(growth, min_growth).map_err(at(min_source))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: usize = 1024 * 1024;

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl FnMut(&str) -> Option<OsString> + 'a {
        move |name| {
            vars.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        }
    }

    fn growth(text: &str) -> Growth {
        Growth::parse(text).unwrap()
    }

    #[test]
    fn unset_means_todays_policy() {
        let settings = GcConfig::default().resolve_with(env(&[])).unwrap();
        assert_eq!(settings, GcSettings::default());
        assert_eq!(settings.growth(), growth("4"));
        assert_eq!(settings.min_growth(), 32 * MIB);
    }

    #[test]
    fn environment_beats_baked_beats_default_per_setting() {
        let baked = GcConfig {
            growth: Some(growth("2")),
            min_growth: Some(8 * MIB),
            ..GcConfig::default()
        };
        let settings = baked.resolve_with(env(&[])).unwrap();
        assert_eq!(
            (settings.growth(), settings.min_growth()),
            (growth("2"), 8 * MIB)
        );

        // Each variable overrides only its own setting.
        let settings = baked.resolve_with(env(&[(GROWTH_ENV_VAR, "0.5")])).unwrap();
        assert_eq!(
            (settings.growth(), settings.min_growth()),
            (growth("0.5"), 8 * MIB)
        );
        let settings = baked
            .resolve_with(env(&[(MIN_GROWTH_ENV_VAR, "2MiB")]))
            .unwrap();
        assert_eq!(
            (settings.growth(), settings.min_growth()),
            (growth("2"), 2 * MIB)
        );

        // Only one baked: the other is the default.
        let half = GcConfig {
            min_growth: Some(4 * MIB),
            ..GcConfig::default()
        };
        let settings = half.resolve_with(env(&[])).unwrap();
        assert_eq!(
            (settings.growth(), settings.min_growth()),
            (growth("4"), 4 * MIB)
        );

        // Empty is unset.
        let settings = baked
            .resolve_with(env(&[(GROWTH_ENV_VAR, "  "), (MIN_GROWTH_ENV_VAR, "")]))
            .unwrap();
        assert_eq!(
            (settings.growth(), settings.min_growth()),
            (growth("2"), 8 * MIB)
        );
    }

    #[test]
    fn a_packed_program_reads_only_the_names_it_carries() {
        let renamed = GcConfig {
            growth_env_var: "ACME_GC_GROWTH".into(),
            min_growth_env_var: "ACME_GC_MIN".into(),
            ..GcConfig::default()
        };
        let settings = renamed
            .resolve_with(env(&[
                (GROWTH_ENV_VAR, "not a number"),
                (MIN_GROWTH_ENV_VAR, "0"),
                ("ACME_GC_GROWTH", "1.5"),
            ]))
            .unwrap();
        assert_eq!(settings.growth(), growth("1.5"));
        assert_eq!(settings.min_growth(), 32 * MIB);
    }

    #[test]
    fn a_bad_variable_is_an_error_that_names_it() {
        for (name, value) in [
            (GROWTH_ENV_VAR, "fast"),
            (GROWTH_ENV_VAR, "0.2"),
            (GROWTH_ENV_VAR, "0"),
            (GROWTH_ENV_VAR, "-1"),
            (GROWTH_ENV_VAR, "1001"),
            (GROWTH_ENV_VAR, "2.555"),
            (MIN_GROWTH_ENV_VAR, "0"),
            (MIN_GROWTH_ENV_VAR, "0MiB"),
            (MIN_GROWTH_ENV_VAR, "512KiB"),
            (MIN_GROWTH_ENV_VAR, "32"),
            (MIN_GROWTH_ENV_VAR, "32MB"),
            (MIN_GROWTH_ENV_VAR, "lots"),
            (MIN_GROWTH_ENV_VAR, "99999999999GiB"),
        ] {
            let error = GcConfig::default()
                .resolve_with(env(&[(name, value)]))
                .expect_err(value);
            assert!(
                matches!(&error, GcConfigError::Value { source_name, .. } if source_name == name),
                "{value}: {error:?}"
            );
            let message = error.to_string();
            assert!(message.contains(name), "{message}");
        }
        let message = GcConfig::default()
            .resolve_with(env(&[(MIN_GROWTH_ENV_VAR, "32MB")]))
            .unwrap_err()
            .to_string();
        assert!(message.contains("MiB"), "{message}");
    }
}
