//! Build-time API-key embedding shared by pack and generate.
use std::ffi::OsString;

use anyhow::{Context, Result};
use btel_settings::artifact::{ArtifactTelemetry, EmbeddedCredential, EmbeddedIngestion};
use clap::Args;

#[derive(Args, Clone, Debug, Default)]
pub struct EmbedTelemetryArgs {
    /// Embed the value of this environment variable as the telemetry API key.
    #[arg(long, value_name = "ENV_VAR", value_parser = parse_envvar, help_heading = "Telemetry options")]
    pub embed_telemetry: Option<String>,
}

fn parse_envvar(name: &str) -> Result<String, String> {
    let mut bytes = name.bytes();
    if matches!(bytes.next(), Some(b'a'..=b'z' | b'A'..=b'Z' | b'_'))
        && bytes.all(|byte| matches!(byte, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_'))
    {
        Ok(name.to_owned())
    } else {
        Err(
            "expected an environment-variable name, for example BAML_WRAPPER_BOUNDARY_API_KEY"
                .into(),
        )
    }
}

impl EmbedTelemetryArgs {
    pub fn apply(&self, policy: &mut ArtifactTelemetry) -> Result<()> {
        self.apply_with(policy, baml_env::os_var)?;
        if self.embed_telemetry.is_some() {
            // Keep the selected build endpoint as the artifact's runtime default.
            let endpoint = bcs_api::Endpoint::with_default(policy.api_url.as_deref())?;
            policy.api_url = Some(endpoint.as_str().to_owned());
        }
        Ok(())
    }

    fn apply_with(
        &self,
        policy: &mut ArtifactTelemetry,
        mut read: impl FnMut(&str) -> Option<OsString>,
    ) -> Result<()> {
        let Some(name) = &self.embed_telemetry else {
            return Ok(());
        };
        let value = read(name)
            .with_context(|| {
                format!("Cannot embed telemetry: environment variable {name} is not set.")
            })?
            .into_string()
            .map_err(|_| {
                anyhow::anyhow!(
                    "Cannot embed telemetry: environment variable {name} is not valid Unicode."
                )
            })?;
        anyhow::ensure!(
            !value.trim().is_empty(),
            "Cannot embed telemetry: environment variable {name} is empty."
        );
        policy.embedded = Some(Box::new(EmbeddedIngestion {
            api_key: EmbeddedCredential::new(value),
        }));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omission_never_reads_the_environment() {
        EmbedTelemetryArgs::default()
            .apply_with(&mut ArtifactTelemetry::default(), |_| {
                panic!("unexpected read")
            })
            .unwrap();
    }

    #[test]
    fn embeds_only_the_named_value_verbatim_and_redacts_debug() {
        let args = EmbedTelemetryArgs {
            embed_telemetry: Some("BAML_WRAPPER_BOUNDARY_API_KEY".into()),
        };
        let mut policy = ArtifactTelemetry::default();
        args.apply_with(&mut policy, |name| {
            assert_eq!(name, "BAML_WRAPPER_BOUNDARY_API_KEY");
            Some(" bdry_secret_embedded ".into())
        })
        .unwrap();
        let key = &policy.embedded.unwrap().api_key;
        assert_eq!(key.expose(), " bdry_secret_embedded ");
        assert_eq!(format!("{key:?}"), "[REDACTED]");
    }

    #[test]
    fn missing_empty_and_non_unicode_values_fail_without_exposing_the_value() {
        let args = EmbedTelemetryArgs {
            embed_telemetry: Some("KEY".into()),
        };
        for (value, expected) in [
            (None, "not set"),
            (Some("".into()), "empty"),
            (Some("  ".into()), "empty"),
        ] {
            let error = args
                .apply_with(&mut ArtifactTelemetry::default(), |_| value.clone())
                .unwrap_err();
            assert_eq!(
                error.to_string(),
                format!("Cannot embed telemetry: environment variable KEY is {expected}.")
            );
        }
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            let error = args
                .apply_with(&mut ArtifactTelemetry::default(), |_| {
                    Some(OsString::from_vec(vec![0xff]))
                })
                .unwrap_err();
            assert_eq!(
                error.to_string(),
                "Cannot embed telemetry: environment variable KEY is not valid Unicode."
            );
        }
    }

    #[test]
    fn requires_a_variable_name() {
        for name in ["KEY", "_KEY_2", "BAML_WRAPPER_BOUNDARY_API_KEY"] {
            assert!(parse_envvar(name).is_ok());
        }
        for name in ["", "1KEY", "KEY=value", "not a name"] {
            assert!(parse_envvar(name).is_err());
        }
    }
}
