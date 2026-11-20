//! The heap collection policy a publisher bakes into a packed program.
//!
//! ```toml
//! [pack.gc]
//! growth = 2          # a multiple of the data that survived; 0.25 to 1000
//! min_growth = "8MiB" # B, KiB, MiB or GiB; at least 1MiB
//!
//! [pack.env_var_names]  # optional: the names the program reads instead
//! BAML_GC_GROWTH = "ACME_GC_GROWTH"
//! BAML_GC_MIN_GROWTH = "ACME_GC_MIN_GROWTH"
//! ```
//!
//! Values stay unset when the manifest omits them, so the runtime's default
//! (or a future default derived from the deployment's memory limit) applies.
//! The program always reads the two variables it carries, standard or
//! renamed: an operator can override the publisher, as with `BAML_TELEMETRY`.
use bex_engine::gc_settings::{
    GROWTH_ENV_VAR, GcConfig, GcSettings, Growth, MIN_GROWTH_ENV_VAR, parse_min_growth,
};
use borsh::{BorshDeserialize, BorshSerialize};

/// Serialized in [`crate::PackEnvelope`]. Raw numbers, validated again when the
/// host turns them into an engine config, since a decoded artifact can bypass
/// [`PackGc::from_manifest`].
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PackGc {
    /// `growth` in hundredths (`2.5` is 250); `None` when not set.
    pub growth_hundredths: Option<u32>,
    /// `min_growth` in bytes; `None` when not set.
    pub min_growth_bytes: Option<u64>,
    /// The variable that overrides `growth` at startup.
    pub growth_env_var: String,
    /// The variable that overrides `min_growth` at startup.
    pub min_growth_env_var: String,
}

impl Default for PackGc {
    fn default() -> Self {
        Self {
            growth_hundredths: None,
            min_growth_bytes: None,
            growth_env_var: GROWTH_ENV_VAR.to_owned(),
            min_growth_env_var: MIN_GROWTH_ENV_VAR.to_owned(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct PackGcError(pub String);

const GC_FIELDS: &[&str] = &["growth", "min_growth"];
/// Names the other `[pack.env_var_names]` bindings use by default; a GC
/// variable may not share one.
const OTHER_BINDINGS: &[&str] = &[
    "BAML_TELEMETRY",
    "BOUNDARY_PROJECT",
    "BOUNDARY_API_KEY",
    "BOUNDARY_API_URL",
];

impl PackGc {
    /// Read `[pack.gc]` and the GC entries of `[pack.env_var_names]`.
    /// Everything else in `[pack]` belongs to `btel_settings`, which also
    /// rejects unknown `[pack]` keys.
    pub fn from_manifest(manifest: &toml::Value) -> Result<Self, PackGcError> {
        let mut gc = Self::default();
        let Some(pack) = manifest.get("pack").and_then(toml::Value::as_table) else {
            return Ok(gc);
        };
        if let Some(table) = pack.get("gc") {
            let table = table
                .as_table()
                .ok_or_else(|| PackGcError("[pack.gc] must be a table.".into()))?;
            if let Some(field) = table.keys().find(|key| !GC_FIELDS.contains(&key.as_str())) {
                return Err(PackGcError(format!(
                    "Unknown setting `pack.gc.{field}` in baml.toml.\n\n  Valid [pack.gc] settings:\n    growth      a multiple of the data that survived, 0.25 to 1000 (default 4)\n    min_growth  a size such as \"32MiB\", at least 1MiB (default 32MiB)"
                )));
            }
            let fail = |key: &str, error: &dyn std::fmt::Display| {
                PackGcError(format!("pack.gc.{key}: {error}"))
            };
            let growth = table
                .get("growth")
                .map(|value| match value {
                    toml::Value::Integer(whole) => u32::try_from(*whole)
                        .ok()
                        .and_then(|whole| whole.checked_mul(100))
                        .map_or_else(
                            || Growth::parse(&whole.to_string()),
                            Growth::from_hundredths,
                        ),
                    toml::Value::Float(value) => Growth::from_f64(*value),
                    _ => Err(bex_engine::gc_settings::GcSettingError::InvalidGrowth {
                        text: value.to_string(),
                    }),
                })
                .transpose()
                .map_err(|error| fail("growth", &error))?;
            let min_growth = table
                .get("min_growth")
                .map(|value| match value {
                    toml::Value::String(text) => parse_min_growth(text),
                    toml::Value::Integer(bytes) => parse_min_growth(&bytes.to_string()),
                    _ => parse_min_growth(&value.to_string()),
                })
                .transpose()
                .map_err(|error| fail("min_growth", &error))?;
            if let Some(min_growth) = min_growth {
                GcSettings::new(growth.unwrap_or(Growth::DEFAULT), min_growth)
                    .map_err(|error| fail("min_growth", &error))?;
            }
            gc.growth_hundredths = growth.map(Growth::hundredths);
            gc.min_growth_bytes = min_growth.map(|bytes| bytes as u64);
        }
        if let Some(names) = pack.get("env_var_names").and_then(toml::Value::as_table) {
            let mut taken: Vec<String> = OTHER_BINDINGS.iter().map(|&name| name.into()).collect();
            taken.extend(
                names
                    .iter()
                    .filter(|(key, _)| *key != GROWTH_ENV_VAR && *key != MIN_GROWTH_ENV_VAR)
                    .filter_map(|(_, value)| value.as_str().map(str::to_owned)),
            );
            for (key, output) in [
                (GROWTH_ENV_VAR, &mut gc.growth_env_var),
                (MIN_GROWTH_ENV_VAR, &mut gc.min_growth_env_var),
            ] {
                let Some(value) = names.get(key) else {
                    continue;
                };
                let name = value
                    .as_str()
                    .filter(|name| valid_env_var(name))
                    .ok_or_else(|| {
                        PackGcError(format!(
                            "pack.env_var_names.{key} must be a valid environment-variable name (for example, \"ACME_GC\"); omit it to use {key}."
                        ))
                    })?;
                name.clone_into(output);
            }
            for name in [&gc.growth_env_var, &gc.min_growth_env_var] {
                if taken.iter().any(|other| other.eq_ignore_ascii_case(name)) {
                    return Err(PackGcError(format!(
                        "pack.env_var_names: `{name}` is already used by another binding; the GC variables need names of their own."
                    )));
                }
            }
            if gc
                .growth_env_var
                .eq_ignore_ascii_case(&gc.min_growth_env_var)
            {
                return Err(PackGcError(format!(
                    "pack.env_var_names: BAML_GC_GROWTH and BAML_GC_MIN_GROWTH both map to `{}`; use distinct names.",
                    gc.growth_env_var
                )));
            }
        }
        Ok(gc)
    }

    /// The engine input for this program. Fails on values no manifest could
    /// have produced.
    pub fn engine_config(&self) -> Result<GcConfig, PackGcError> {
        let corrupt = |error: &dyn std::fmt::Display| {
            PackGcError(format!("packed program has an invalid GC setting: {error}"))
        };
        let growth = self
            .growth_hundredths
            .map(Growth::from_hundredths)
            .transpose()
            .map_err(|error| corrupt(&error))?;
        let min_growth = self
            .min_growth_bytes
            .map(|bytes| {
                let bytes = usize::try_from(bytes).unwrap_or(usize::MAX);
                GcSettings::new(growth.unwrap_or(Growth::DEFAULT), bytes).map(|_| bytes)
            })
            .transpose()
            .map_err(|error| corrupt(&error))?;
        for name in [&self.growth_env_var, &self.min_growth_env_var] {
            if !valid_env_var(name) {
                return Err(corrupt(&format_args!("`{name}` is not a variable name")));
            }
        }
        Ok(GcConfig {
            growth,
            min_growth,
            growth_env_var: self.growth_env_var.clone(),
            min_growth_env_var: self.min_growth_env_var.clone(),
        })
    }
}

fn valid_env_var(value: &str) -> bool {
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z' | b'A'..=b'Z' | b'_'))
        && bytes.all(|byte| matches!(byte, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_'))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

    const MIB: u64 = 1024 * 1024;

    fn parse(text: &str) -> Result<PackGc, PackGcError> {
        PackGc::from_manifest(&text.parse::<toml::Value>().unwrap())
    }

    #[test]
    fn omitted_settings_stay_unset() {
        for manifest in ["", "[package]\nname = \"x\"", "[pack]\nhttp_bodies = false"] {
            assert_eq!(parse(manifest).unwrap(), PackGc::default());
        }
    }

    #[test]
    fn values_and_formats() {
        for (growth, hundredths) in [("4", 400), ("2.5", 250), ("0.25", 25), ("1000", 100_000)] {
            let gc = parse(&format!("[pack.gc]\ngrowth = {growth}")).unwrap();
            assert_eq!(gc.growth_hundredths, Some(hundredths), "{growth}");
            assert_eq!(gc.min_growth_bytes, None);
        }
        for (size, bytes) in [
            ("\"32MiB\"", 32 * MIB),
            ("\"8 mib\"", 8 * MIB),
            ("\"1024KiB\"", MIB),
            ("\"2GiB\"", 2048 * MIB),
            ("\"1048576B\"", MIB),
            ("1048576", MIB),
        ] {
            let gc = parse(&format!("[pack.gc]\nmin_growth = {size}")).unwrap();
            assert_eq!(gc.min_growth_bytes, Some(bytes), "{size}");
        }
    }

    #[test]
    fn values_that_cannot_work_fail_the_pack() {
        for (setting, expect) in [
            ("growth = 0.2", "0.25 to 1000"),
            ("growth = 0", "0.25 to 1000"),
            ("growth = -1", "growth"),
            ("growth = 1001", "0.25 to 1000"),
            ("growth = 2.555", "two decimals"),
            ("growth = \"2\"", "growth"),
            ("growth = nan", "growth"),
            ("min_growth = 0", "1 MiB"),
            ("min_growth = 32", "add a unit"),
            ("min_growth = \"0MiB\"", "at least 1 MiB"),
            ("min_growth = \"512KiB\"", "at least 1 MiB"),
            ("min_growth = \"32MB\"", "MiB"),
            ("min_growth = \"32 megs\"", "not a size"),
            ("min_growth = 1.5", "not a size"),
            (
                "min_budget = \"32MiB\"",
                "Unknown setting `pack.gc.min_budget`",
            ),
        ] {
            let error = parse(&format!("[pack.gc]\n{setting}"))
                .expect_err(setting)
                .0;
            assert!(error.contains(expect), "{setting}: {error}");
        }
        assert!(parse("[pack]\ngc = 4").unwrap_err().0.contains("table"));
    }

    #[test]
    fn variables_can_be_renamed_but_not_shared() {
        let gc = parse(
            "[pack.env_var_names]\nBAML_GC_GROWTH = \"ACME_GC_GROWTH\"\nBAML_GC_MIN_GROWTH = \"ACME_GC_MIN\"",
        )
        .unwrap();
        assert_eq!(gc.growth_env_var, "ACME_GC_GROWTH");
        assert_eq!(gc.min_growth_env_var, "ACME_GC_MIN");
        for names in [
            "BAML_GC_GROWTH = \"1BAD\"",
            "BAML_GC_GROWTH = false",
            "BAML_GC_GROWTH = \"BAML_TELEMETRY\"",
            "BAML_GC_GROWTH = \"BAML_GC_MIN_GROWTH\"",
            "BAML_GC_MIN_GROWTH = \"BOUNDARY_API_KEY\"",
            "BAML_TELEMETRY = \"ACME\"\nBAML_GC_GROWTH = \"ACME\"",
        ] {
            assert!(
                parse(&format!("[pack.env_var_names]\n{names}")).is_err(),
                "{names}"
            );
        }
    }

    /// What the host does with the envelope: the baked values reach the heap's
    /// settings, the carried names override them, and anything else is ignored.
    #[test]
    fn the_host_reads_baked_values_under_the_carried_names() {
        let gc = parse(
            "[pack.gc]\ngrowth = 1.5\nmin_growth = \"8MiB\"\n[pack.env_var_names]\nBAML_GC_GROWTH = \"ACME_GC_GROWTH\"",
        )
        .unwrap();
        let decoded = borsh::from_slice::<PackGc>(&borsh::to_vec(&gc).unwrap()).unwrap();
        assert_eq!(decoded, gc);
        let config = decoded.engine_config().unwrap();
        let resolve = |vars: &[(&str, &str)]| {
            let vars: Vec<(String, String)> = vars
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect();
            config.resolve_with(|name| {
                vars.iter()
                    .find(|(key, _)| key == name)
                    .map(|(_, value)| OsString::from(value))
            })
        };
        let settings = resolve(&[("BAML_GC_GROWTH", "nonsense")]).unwrap();
        assert_eq!(settings.growth(), Growth::parse("1.5").unwrap());
        assert_eq!(settings.min_growth(), 8 * 1024 * 1024);
        let settings = resolve(&[("ACME_GC_GROWTH", "3"), ("BAML_GC_MIN_GROWTH", "2MiB")]).unwrap();
        assert_eq!(settings.growth(), Growth::parse("3").unwrap());
        assert_eq!(settings.min_growth(), 2 * 1024 * 1024);
        assert!(resolve(&[("ACME_GC_GROWTH", "0.1")]).is_err());
    }

    #[test]
    fn a_corrupt_envelope_is_refused() {
        for gc in [
            PackGc {
                growth_hundredths: Some(10),
                ..PackGc::default()
            },
            PackGc {
                min_growth_bytes: Some(0),
                ..PackGc::default()
            },
            PackGc {
                growth_env_var: "NOT A NAME".into(),
                ..PackGc::default()
            },
        ] {
            assert!(gc.engine_config().is_err(), "{gc:?}");
        }
    }
}
