//! The variable a packed program reads its `log.*` filter from.
//!
//! ```toml
//! [pack.env_var_names]  # optional: the name the program reads instead
//! BAML_LOG = "ACME_LOG"
//! ```
//!
//! The program always reads the variable it carries, standard or renamed, as
//! with the GC variables: the publisher chooses the name, the operator the
//! filter.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::{
    log_output::LOG_ENV,
    pack_env::{taken_names, valid_env_var},
};

/// Serialized in [`crate::PackEnvelope`].
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PackLog {
    /// Checked again by [`Self::env_var`], since a decoded artifact can
    /// bypass [`Self::from_manifest`].
    env_var: String,
}

impl Default for PackLog {
    fn default() -> Self {
        Self {
            env_var: LOG_ENV.to_owned(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct PackLogError(pub String);

impl PackLog {
    /// Read the `BAML_LOG` entry of `[pack.env_var_names]`. `btel_settings`
    /// owns the table and rejects its unknown keys.
    pub fn from_manifest(manifest: &toml::Value) -> Result<Self, PackLogError> {
        let names = manifest
            .get("pack")
            .and_then(|pack| pack.get("env_var_names"))
            .and_then(toml::Value::as_table);
        let env_var = match names.and_then(|names| names.get(LOG_ENV)) {
            None => LOG_ENV,
            Some(value) => value
                .as_str()
                .filter(|name| valid_env_var(name))
                .ok_or_else(|| {
                    PackLogError(format!(
                        "pack.env_var_names.{LOG_ENV} must be a valid environment-variable name (for example, \"ACME_LOG\"); omit it to use {LOG_ENV}."
                    ))
                })?,
        };
        // Checked even when not renamed: another binding may be renamed to
        // the standard name.
        if taken_names(names, &[LOG_ENV])
            .iter()
            .any(|taken| taken.eq_ignore_ascii_case(env_var))
        {
            return Err(PackLogError(format!(
                "pack.env_var_names: `{env_var}` is used by another binding; the {LOG_ENV} variable needs a name of its own (for example, \"ACME_LOG\")."
            )));
        }
        Ok(Self {
            env_var: env_var.to_owned(),
        })
    }

    /// The variable to read. Fails on a name no manifest could have produced.
    pub fn env_var(&self) -> Result<&str, PackLogError> {
        if valid_env_var(&self.env_var) {
            Ok(&self.env_var)
        } else {
            Err(PackLogError(format!(
                "packed program has an invalid log setting: `{}` is not a variable name",
                self.env_var
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<PackLog, PackLogError> {
        PackLog::from_manifest(&text.parse::<toml::Value>().unwrap())
    }

    #[test]
    fn the_standard_name_unless_renamed() {
        for manifest in [
            "",
            "[pack]\nhttp_bodies = false",
            "[pack.env_var_names]\nBAML_TELEMETRY = \"ACME_TELEMETRY\"",
        ] {
            assert_eq!(
                parse(manifest).unwrap().env_var(),
                Ok(LOG_ENV),
                "{manifest}"
            );
        }
        let renamed = parse("[pack.env_var_names]\nBAML_LOG = \"ACME_LOG\"").unwrap();
        assert_eq!(renamed.env_var(), Ok("ACME_LOG"));
    }

    #[test]
    fn a_name_must_be_a_variable_name() {
        for value in ["\"\"", "\"1LOG\"", "\"ACME-LOG\"", "3"] {
            let error = parse(&format!("[pack.env_var_names]\nBAML_LOG = {value}")).unwrap_err();
            assert_eq!(
                error.0,
                "pack.env_var_names.BAML_LOG must be a valid environment-variable name (for example, \"ACME_LOG\"); omit it to use BAML_LOG.",
                "{value}"
            );
        }
    }

    #[test]
    fn a_name_no_other_binding_uses() {
        for (names, taken) in [
            // Another binding's standard name, ignoring case.
            ("BAML_LOG = \"BAML_TELEMETRY\"", "BAML_TELEMETRY"),
            ("BAML_LOG = \"baml_gc_growth\"", "baml_gc_growth"),
            // A variable the host reads under a fixed name.
            ("BAML_LOG = \"BOUNDARY_API_URL\"", "BOUNDARY_API_URL"),
            ("BAML_LOG = \"BAML_CONTEXT\"", "BAML_CONTEXT"),
            ("BAML_LOG = \"baml_home\"", "baml_home"),
            (
                "BAML_LOG = \"BAML_COMPILED_BODIES\"",
                "BAML_COMPILED_BODIES",
            ),
            // Another binding renamed to this one's name.
            (
                "BAML_LOG = \"ACME_LOG\"\nBAML_TELEMETRY = \"ACME_LOG\"",
                "ACME_LOG",
            ),
            // Another binding renamed to this one's standard name.
            ("BAML_TELEMETRY = \"BAML_LOG\"", "BAML_LOG"),
            ("BAML_GC_GROWTH = \"baml_log\"", "BAML_LOG"),
        ] {
            let error = parse(&format!("[pack.env_var_names]\n{names}")).unwrap_err();
            assert_eq!(
                error.0,
                format!(
                    "pack.env_var_names: `{taken}` is used by another binding; the BAML_LOG variable needs a name of its own (for example, \"ACME_LOG\")."
                ),
                "{names}"
            );
        }
    }

    #[test]
    fn a_decoded_name_is_checked_again() {
        let corrupt = PackLog {
            env_var: "not a name".to_owned(),
        };
        assert!(corrupt.env_var().is_err());
    }
}
