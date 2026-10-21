//! Build-time telemetry provisioning shared by pack and generate. Private minting
//! credentials remain in the build process; artifacts receive only public ingestion.
use anyhow::{Context, Result, bail};
use btel_settings::artifact::{
    ArtifactTelemetry, BytecodeDigest, EmbeddedIngestion, IngestionDestination, PublicCredential,
};
use clap::Args;

#[derive(Args, Clone, Debug, Default)]
pub struct EmbedTelemetryArgs {
    /// Mint and embed a public ingestion credential for this build.
    #[arg(long, help_heading = "Telemetry options")]
    pub embed_telemetry: bool,
    /// Existing environment to bind the embedded credential to.
    #[arg(
        long,
        value_name = "NAME",
        requires = "embed_telemetry",
        help_heading = "Telemetry options"
    )]
    pub telemetry_environment: Option<String>,
}

pub struct Provisioning {
    client: bcs_api::Client,
    authorization: bcs_api::credentials::RequestAuthorization,
    project: String,
    environment: String,
    context: bcs_api::diagnostics::Context,
}

impl EmbedTelemetryArgs {
    pub fn prepare(
        &self,
        policy: &mut ArtifactTelemetry,
        default_environment: Option<&str>,
        producer: &str,
    ) -> Result<Option<Provisioning>> {
        if !self.embed_telemetry {
            return Ok(None);
        }
        let environment = self.telemetry_environment.as_deref().or(default_environment)
            .filter(|value| !value.is_empty())
            .with_context(|| format!("Embedding telemetry requires an existing environment. Pass `--telemetry-environment=<name>` or set `{producer}.telemetry_environment` in baml.toml."))?.to_owned();
        let project = baml_env::string_var("BOUNDARY_PROJECT")?.or_else(|| policy.project.clone())
            .filter(|value| !value.is_empty())
            .context("Embedding telemetry requires a project. Set `[boundary].project = \"org/project\"` in baml.toml or set BOUNDARY_PROJECT=org/project.")?;
        if project.split('/').count() != 2 || project.split('/').any(str::is_empty) {
            bail!(
                "BOUNDARY_PROJECT or boundary.project must have the form org_handle/project_name."
            );
        }
        let endpoint = bcs_api::Endpoint::with_default(policy.api_url.as_deref())?;
        let key = baml_env::string_var("BOUNDARY_API_KEY")?;
        let source = if key.is_some() {
            bcs_api::diagnostics::CredentialSource::ApiKey
        } else {
            bcs_api::diagnostics::CredentialSource::SavedLogin
        };
        let context = bcs_api::diagnostics::Context {
            endpoint: Some(endpoint.clone()),
            source,
            operation: bcs_api::diagnostics::Operation::MintBuildToken,
        };
        let credential = match key {
            Some(key) => {
                if key == bcs_api::credentials::LOCAL_API_KEY
                    || key.starts_with(bcs_api::credentials::PUBLIC_PREFIX)
                {
                    bail!(
                        "Embedding telemetry requires a Boundary API key with mint permission, or an administrator's `baml auth login` session. BOUNDARY_API_KEY=local and public ingestion credentials cannot mint tokens."
                    );
                }
                bcs_api::Secret::new(key)
            }
            None => bcs_api::Store::new(&endpoint)
                .and_then(|store| store.read())
                .and_then(|saved| {
                    saved
                        .map(|saved| saved.refresh_token)
                        .ok_or(bcs_api::Error::MissingCredentials)
                })
                .map_err(|error| {
                    context.report(error, bcs_api::diagnostics::Outcome::ArtifactBuildFailed)
                })?,
        };
        // The chosen build endpoint and project are defaults for the shipped artifact.
        policy.project = Some(project.clone());
        policy.api_url = Some(endpoint.as_str().to_owned());
        let authorization = bcs_api::credentials::RequestAuthorization {
            authentication: bcs_api::credentials::Authentication::shared(
                endpoint.as_str(),
                credential,
            ),
            target: None,
        };
        Ok(Some(Provisioning {
            client: bcs_api::Client::new(endpoint)?,
            authorization,
            project,
            environment,
            context,
        }))
    }
}

impl Provisioning {
    pub fn mint(&self, digest: BytecodeDigest) -> Result<EmbeddedIngestion> {
        let wire_digest = bcs_api::builds::BytecodeDigest {
            version: digest.version,
            algorithm: digest.algorithm.clone(),
            value: digest.value.clone(),
        };
        let request = bcs_api::builds::MintBuildToken {
            target: bcs_api::builds::BuildTarget::Handle {
                project: &self.project,
                environment: &self.environment,
            },
            bytecode_digest: &wire_digest,
        };
        let minted = self
            .client
            .mint_build_token(&self.authorization, uuid::Uuid::new_v4(), &request)
            .map_err(|error| {
                self.context
                    .report(error, bcs_api::diagnostics::Outcome::ArtifactBuildFailed)
            })?;
        Ok(EmbeddedIngestion {
            token: PublicCredential::new(minted.token.expose().to_owned()),
            token_id: minted.token_id,
            build_id: minted.build_id,
            product_id: minted.product_id,
            destination: IngestionDestination {
                org_id: minted.destination.org_id,
                project_id: minted.destination.project_id,
                environment_id: minted.destination.environment_id,
            },
            bytecode_digest: digest,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn environment_default_does_not_enable_minting_and_missing_environment_explains_both_inputs() {
        assert!(
            EmbedTelemetryArgs::default()
                .prepare(&mut ArtifactTelemetry::default(), Some("staging"), "pack")
                .unwrap()
                .is_none()
        );
        for producer in ["pack", "bridge"] {
            let args = EmbedTelemetryArgs {
                embed_telemetry: true,
                telemetry_environment: None,
            };
            let error = args
                .prepare(&mut ArtifactTelemetry::default(), None, producer)
                .err()
                .unwrap();
            assert_eq!(
                error.to_string(),
                format!(
                    r#"Embedding telemetry requires an existing environment. Pass `--telemetry-environment=<name>` or set `{producer}.telemetry_environment` in baml.toml."#
                )
            );
        }
    }
}
