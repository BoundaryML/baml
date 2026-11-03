//! Build-time API-key embedding shared by pack and generate.
use std::ffi::OsString;

use anyhow::{Context, Result};
use btel_settings::artifact::{
    ArtifactKind, ArtifactTelemetry, BuildTelemetry, EmbeddedCredential, EmbeddedIngestion,
};
use clap::Args;

#[derive(Args, Clone, Debug, Default)]
pub struct EmbedTelemetryArgs {
    /// Verify exclusive write access to the configured environment and embed its API key.
    #[arg(long, help_heading = "Telemetry options")]
    pub embed_telemetry: bool,
}

impl EmbedTelemetryArgs {
    pub fn apply(
        &self,
        policy: &mut ArtifactTelemetry,
        manifest: &toml::Value,
        kind: ArtifactKind,
    ) -> Result<()> {
        if !self.embed_telemetry {
            return Ok(());
        }
        let build = BuildTelemetry::from_manifest(manifest, kind)?;
        let (setting, environment_setting) = match kind {
            ArtifactKind::Pack => (
                "pack.build.embed_telemetry_api_key_from_env_var",
                "pack.build.embed_telemetry_env",
            ),
            ArtifactKind::Bridge => (
                "bridge.build.embed_telemetry_api_key_from_env_var",
                "bridge.build.embed_telemetry_env",
            ),
        };
        let project = manifest.get("boundary").and_then(|v| v.get("project"))
            .and_then(toml::Value::as_str).context("Cannot embed telemetry: set boundary.project to org_handle/project_name in baml.toml.")?;
        let (organization, project_name) = project.split_once('/').filter(|(org, name)| {
            !org.is_empty() && !name.trim().is_empty() && !name.contains('/') &&
                !org.chars().any(char::is_whitespace)
        }).context("Cannot embed telemetry: boundary.project must be org_handle/project_name; correct the manifest or supply a key owned by that project.")?;
        let environment = build.expected_environment.as_deref().filter(|v| !v.trim().is_empty())
            .with_context(|| format!("Cannot embed telemetry: set {environment_setting} in baml.toml to the expected environment for exclusive write access."))?;
        let mut candidate = policy.clone();
        self.apply_with(
            &mut candidate,
            build.api_key_from_env_var.as_deref(),
            setting,
            baml_env::os_var,
        )?;
        let embedded = candidate
            .embedded
            .as_deref()
            .expect("embedding selects a credential");
        let endpoint = bcs_api::Endpoint::with_default(candidate.api_url.as_deref())?;
        let client = bcs_api::Client::new(endpoint.clone())?;
        let key = bcs_api::Secret::new(embedded.api_key.expose().to_owned());
        let verified = client
            .inspect_api_key(&key)
            .context("Cannot embed telemetry: API-key verification failed")?;
        validate_key(&verified, organization, project_name, environment).map_err(|error| {
            anyhow::anyhow!(error.to_string().replace(key.expose(), "[REDACTED]"))
        })?;
        candidate.api_url = Some(endpoint.as_str().to_owned());
        *policy = candidate;
        Ok(())
    }

    fn apply_with(
        &self,
        policy: &mut ArtifactTelemetry,
        configured_name: Option<&str>,
        setting: &str,
        mut read: impl FnMut(&str) -> Option<OsString>,
    ) -> Result<()> {
        if !self.embed_telemetry {
            return Ok(());
        }
        let name = configured_name.with_context(|| {
            format!(
                "Cannot embed telemetry: set {setting} in baml.toml before using --embed-telemetry."
            )
        })?;
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
        anyhow::ensure!(
            value != bcs_api::credentials::LOCAL_API_KEY,
            "Cannot embed telemetry: local is not a Boundary API key."
        );
        policy.embedded = Some(Box::new(EmbeddedIngestion {
            api_key: EmbeddedCredential::new(value),
        }));
        Ok(())
    }
}

fn validate_key(
    key: &bcs_api::api_keys::VerifiedApiKey,
    org: &str,
    project: &str,
    environment: &str,
) -> Result<()> {
    use bcs_api::api_keys::PermissionTarget;
    let verified_org = key.organization.slug.as_deref().unwrap_or("<unavailable>");
    anyhow::ensure!(
        verified_org == org
            && key.project.name == project
            && !key.organization.id.is_empty()
            && !key.project.id.is_empty(),
        "Cannot embed telemetry: configured project {org}/{project} differs from verified owning project {verified_org}/{}. Correct boundary.project in the manifest or supply a key owned by that project.",
        key.project.name
    );
    anyhow::ensure!(
        key.permissions.query.is_empty(),
        "Cannot embed telemetry: the key must have exclusive write access to environment {environment}, with no query or other operation permissions."
    );
    let [PermissionTarget::Environment { environment_id }] = key.permissions.ingest.as_slice()
    else {
        anyhow::bail!(
            "Cannot embed telemetry: the key must have exactly one environment ingestion target for {environment}."
        );
    };
    let [resolved] = key.ingest_environments.as_slice() else {
        anyhow::bail!(
            "Cannot embed telemetry: resolved ingestion environment metadata is missing or inconsistent for {environment}."
        );
    };
    anyhow::ensure!(
        !environment_id.is_empty()
            && resolved.id == *environment_id
            && resolved.name == environment
            && resolved.project_id == key.project.id
            && resolved.organization_id == key.organization.id,
        "Cannot embed telemetry: the key's resolved ingestion environment does not match {org}/{project}/{environment} or its owning project and organization."
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SETTING: &str = "pack.build.embed_telemetry_api_key_from_env_var";

    #[test]
    fn omission_never_reads_the_environment() {
        EmbedTelemetryArgs::default()
            .apply_with(
                &mut ArtifactTelemetry::default(),
                Some("BUILD_KEY"),
                SETTING,
                |_| panic!("unexpected read"),
            )
            .unwrap();
    }

    #[test]
    fn flag_reads_only_the_manifest_variable_and_redacts_debug() {
        let args = EmbedTelemetryArgs {
            embed_telemetry: true,
        };
        let mut policy = ArtifactTelemetry::default();
        args.apply_with(&mut policy, Some("BUILD_KEY"), SETTING, |name| {
            assert_eq!(name, "BUILD_KEY");
            Some(" bdry_secret_embedded ".into())
        })
        .unwrap();
        let key = &policy.embedded.unwrap().api_key;
        assert_eq!(key.expose(), " bdry_secret_embedded ");
        assert_eq!(format!("{key:?}"), "[REDACTED]");
    }

    #[test]
    fn flag_requires_manifest_configuration_before_reading_credentials() {
        let args = EmbedTelemetryArgs {
            embed_telemetry: true,
        };
        for setting in [SETTING, "bridge.build.embed_telemetry_api_key_from_env_var"] {
            let error = args
                .apply_with(&mut ArtifactTelemetry::default(), None, setting, |_| {
                    panic!("unexpected read")
                })
                .unwrap_err();
            assert_eq!(
                error.to_string(),
                format!(
                    "Cannot embed telemetry: set {setting} in baml.toml before using --embed-telemetry."
                )
            );
        }
    }

    #[test]
    fn missing_empty_and_non_unicode_values_fail_without_exposing_the_value() {
        let args = EmbedTelemetryArgs {
            embed_telemetry: true,
        };
        for (value, expected) in [
            (None, "not set"),
            (Some("".into()), "empty"),
            (Some("  ".into()), "empty"),
        ] {
            let error = args
                .apply_with(
                    &mut ArtifactTelemetry::default(),
                    Some("KEY"),
                    SETTING,
                    |_| value.clone(),
                )
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
                .apply_with(
                    &mut ArtifactTelemetry::default(),
                    Some("KEY"),
                    SETTING,
                    |_| Some(OsString::from_vec(vec![0xff])),
                )
                .unwrap_err();
            assert_eq!(
                error.to_string(),
                "Cannot embed telemetry: environment variable KEY is not valid Unicode."
            );
        }
    }
}
