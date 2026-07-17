//! Representation-independent validation for LLM client options.
//!
//! The compiler and runtime represent client options differently, so this
//! module validates only the semantic facts they share. Callers are
//! responsible for attaching source spans or runtime client names to errors.

use std::fmt;

/// Whether the options needed for provider-specific validation are present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientOptionsPresence<'a> {
    pub provider: &'a str,
    pub base_url: bool,
    pub resource_name: bool,
    pub deployment_id: bool,
}

/// A provider-specific client options constraint that was not satisfied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientOptionsValidationError {
    AzureOpenAiMissingEndpoint {
        missing: AzureOpenAiMissingEndpointOptions,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AzureOpenAiMissingEndpointOptions {
    ResourceNameAndDeploymentId,
    ResourceName,
    DeploymentId,
}

impl fmt::Display for ClientOptionsValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AzureOpenAiMissingEndpoint { missing } => {
                let missing = match missing {
                    AzureOpenAiMissingEndpointOptions::ResourceNameAndDeploymentId => {
                        "resource_name and deployment_id"
                    }
                    AzureOpenAiMissingEndpointOptions::ResourceName => "resource_name",
                    AzureOpenAiMissingEndpointOptions::DeploymentId => "deployment_id",
                };
                write!(
                    f,
                    "azure-openai requires either base_url or both resource_name and deployment_id (missing: {missing})"
                )
            }
        }
    }
}

impl std::error::Error for ClientOptionsValidationError {}

/// Validate provider-specific constraints shared by compiled and runtime clients.
pub fn validate_client_options(
    options: ClientOptionsPresence<'_>,
) -> Result<(), ClientOptionsValidationError> {
    if options.provider == "azure-openai" && !options.base_url {
        let missing = match (options.resource_name, options.deployment_id) {
            (false, false) => AzureOpenAiMissingEndpointOptions::ResourceNameAndDeploymentId,
            (false, true) => AzureOpenAiMissingEndpointOptions::ResourceName,
            (true, false) => AzureOpenAiMissingEndpointOptions::DeploymentId,
            (true, true) => return Ok(()),
        };
        return Err(ClientOptionsValidationError::AzureOpenAiMissingEndpoint { missing });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn azure(
        base_url: bool,
        resource_name: bool,
        deployment_id: bool,
    ) -> ClientOptionsPresence<'static> {
        ClientOptionsPresence {
            provider: "azure-openai",
            base_url,
            resource_name,
            deployment_id,
        }
    }

    #[test]
    fn azure_accepts_either_endpoint_form() {
        assert!(validate_client_options(azure(true, false, false)).is_ok());
        assert!(validate_client_options(azure(false, true, true)).is_ok());
    }

    #[test]
    fn azure_reports_the_missing_endpoint_fields() {
        for (options, missing) in [
            (
                azure(false, false, false),
                "resource_name and deployment_id",
            ),
            (azure(false, false, true), "resource_name"),
            (azure(false, true, false), "deployment_id"),
        ] {
            let error = validate_client_options(options).unwrap_err();
            assert_eq!(
                error.to_string(),
                format!(
                    "azure-openai requires either base_url or both resource_name and deployment_id (missing: {missing})"
                )
            );
        }
    }

    #[test]
    fn other_providers_do_not_use_azure_constraints() {
        let options = ClientOptionsPresence {
            provider: "openai",
            base_url: false,
            resource_name: false,
            deployment_id: false,
        };
        assert!(validate_client_options(options).is_ok());
    }
}
