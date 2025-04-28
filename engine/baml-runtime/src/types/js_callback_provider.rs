use std::sync::OnceLock;

use derive_new::new;
use thiserror::Error;

#[derive(Debug, Error, Clone)]
/// For baml-src-reader and aws-cred-provider, provide a statically defined type which is Send + Sync
/// anyhow::Error is not Send + Sync, so it's convoluted to use it in this callback context
pub enum RuntimeCallbackError {
    #[error("Failed to send cred request across WASM bridge: {0}")]
    SendError(String),

    #[error("Failed to recv cred response across WASM bridge: {0}")]
    RecvError(String),

    #[error("Failed to load aws creds: {0}")]
    AwsCredProviderError(String),

    #[error("BAML internal error - credential provider bridges not initialized")]
    NoCredProviderBridge,
}

static_assertions::assert_impl_all!(RuntimeCallbackError: Send, Sync);

pub type RuntimeCallbackResult<T> = Result<T, RuntimeCallbackError>;

static JS_CALLBACK_PROVIDER_SINGLETON: OnceLock<JsCallbackProvider> = OnceLock::new();

pub fn get_remote_cred_provider() -> Result<&'static JsCallbackProvider, RuntimeCallbackError> {
    JS_CALLBACK_PROVIDER_SINGLETON
        .get()
        .ok_or(RuntimeCallbackError::NoCredProviderBridge)
}

pub fn set_remote_cred_provider(aws_cred_provider: JsCallbackProvider) {
    JS_CALLBACK_PROVIDER_SINGLETON.set(aws_cred_provider);
}

#[derive(serde::Deserialize, Debug, Clone, Eq, PartialEq)]
pub enum AwsCredResult {
    #[serde(rename = "error", rename_all = "camelCase")]
    Err { name: String, message: String },

    #[serde(rename = "ok", rename_all = "camelCase")]
    /// This is 1:1 with AwsCredentialIdentity in @smithy/types
    /// https://docs.aws.amazon.com/AWSJavaScriptSDK/v3/latest/Package/-smithy-types/Interface/AwsCredentialIdentity/
    Ok {
        access_key_id: String,
        secret_access_key: String,
        session_token: Option<String>,
        credential_scope: Option<String>,
        expiration: Option<String>,
        account_id: Option<String>,
    },
}

#[derive(serde::Deserialize, Debug, Clone, Eq, PartialEq)]
pub enum GcpCredResult {
    #[serde(rename = "error", rename_all = "camelCase")]
    Err { name: String, message: String },

    #[serde(rename = "ok", rename_all = "camelCase")]
    Ok {
        access_token: String,
        project_id: String,
    },
}

#[derive(new)]
pub struct JsCallbackProvider {
    aws_req_tx: tokio::sync::mpsc::Sender<Option<String>>,
    aws_resp_rx: tokio::sync::broadcast::Receiver<RuntimeCallbackResult<AwsCredResult>>,
    gcp_req_tx: tokio::sync::mpsc::Sender<Option<String>>,
    gcp_resp_rx: tokio::sync::broadcast::Receiver<RuntimeCallbackResult<GcpCredResult>>,
}

impl JsCallbackProvider {
    pub async fn aws_req(
        &self,
        profile_name: Option<String>,
    ) -> RuntimeCallbackResult<AwsCredResult> {
        let req_tx = self.aws_req_tx.clone();
        let mut resp_rx = self.aws_resp_rx.resubscribe();

        if let Err(e) = req_tx.send(profile_name).await {
            log::error!(
                "Failed to send AWS cred request across WASM bridge: {:?}",
                e
            );
            return Err(RuntimeCallbackError::SendError(e.to_string()));
        };
        let creds = match resp_rx.recv().await {
            Ok(Ok(creds)) => creds,
            Ok(Err(e)) => {
                log::error!("Error in AWS cred provider: {:?}", e);
                return Err(e);
            }
            Err(e) => {
                log::error!(
                    "Failed to recv AWS cred response across WASM bridge: {:?}",
                    e
                );
                return Err(RuntimeCallbackError::RecvError(e.to_string()));
            }
        };

        Ok(creds)
    }

    pub async fn gcp_req(&self) -> RuntimeCallbackResult<GcpCredResult> {
        let req_tx = self.gcp_req_tx.clone();
        let mut resp_rx = self.gcp_resp_rx.resubscribe();

        if let Err(e) = req_tx.send(None).await {
            log::error!(
                "Failed to send GCP cred request across WASM bridge: {:?}",
                e
            );
            return Err(RuntimeCallbackError::SendError(e.to_string()));
        };
        let creds = match resp_rx.recv().await {
            Ok(Ok(creds)) => creds,
            Ok(Err(e)) => {
                log::error!("Error in GCP cred provider: {:?}", e);
                return Err(e);
            }
            Err(e) => {
                log::error!(
                    "Failed to recv GCP cred response across WASM bridge: {:?}",
                    e
                );
                return Err(RuntimeCallbackError::RecvError(e.to_string()));
            }
        };

        Ok(creds)
    }
}

impl Clone for JsCallbackProvider {
    fn clone(&self) -> Self {
        Self {
            aws_req_tx: self.aws_req_tx.clone(),
            aws_resp_rx: self.aws_resp_rx.resubscribe(),
            gcp_req_tx: self.gcp_req_tx.clone(),
            gcp_resp_rx: self.gcp_resp_rx.resubscribe(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_aws_cred_result_deserialize_ok() {
        let json = json!({
            "ok": {
                "accessKeyId": "AKIATEST",
                "secretAccessKey": "secret123",
                "sessionToken": "token123",
                "credentialScope": "aws/scope",
                "expiration": "2024-03-21T00:00:00Z",
                "accountId": "123456789"
            }
        });

        let result: AwsCredResult =
            serde_json::from_value(json).expect("Failed to deserialize AWS credentials result");

        assert_eq!(
            result,
            AwsCredResult::Ok {
                access_key_id: "AKIATEST".into(),
                secret_access_key: "secret123".into(),
                session_token: Some("token123".into()),
                credential_scope: Some("aws/scope".into()),
                expiration: Some("2024-03-21T00:00:00Z".into()),
                account_id: Some("123456789".into()),
            }
        );
    }

    #[test]
    fn test_aws_cred_result_deserialize_ok_minimal() {
        let json = json!({
            "ok": {
                "accessKeyId": "AKIATEST",
                "secretAccessKey": "secret123"
            }
        });

        let result: AwsCredResult = serde_json::from_value(json)
            .expect("Failed to deserialize minimal AWS credentials result");

        assert_eq!(
            result,
            AwsCredResult::Ok {
                access_key_id: "AKIATEST".into(),
                secret_access_key: "secret123".into(),
                session_token: None,
                credential_scope: None,
                expiration: None,
                account_id: None,
            }
        );
    }

    #[test]
    fn test_aws_cred_result_deserialize_error() {
        let json = json!({
            "error": {
                "name": "CredentialError",
                "message": "Failed to load credentials"
            }
        });

        let result: AwsCredResult =
            serde_json::from_value(json).expect("Failed to deserialize AWS credentials error");

        assert_eq!(
            result,
            AwsCredResult::Err {
                name: "CredentialError".into(),
                message: "Failed to load credentials".into(),
            }
        );
    }

    #[test]
    fn test_gcp_cred_result_deserialize_ok() {
        let json = json!({
            "ok": {
                "accessToken": "ya29.token",
                "projectId": "my-project-123"
            }
        });

        let result: GcpCredResult =
            serde_json::from_value(json).expect("Failed to deserialize GCP credentials result");

        assert_eq!(
            result,
            GcpCredResult::Ok {
                access_token: "ya29.token".into(),
                project_id: "my-project-123".into(),
            }
        );
    }

    #[test]
    fn test_gcp_cred_result_deserialize_error() {
        let json = json!({
            "error": {
                "name": "GcpCredentialError",
                "message": "Failed to get GCP credentials"
            }
        });

        let result: GcpCredResult =
            serde_json::from_value(json).expect("Failed to deserialize GCP credentials error");

        assert_eq!(
            result,
            GcpCredResult::Err {
                name: "GcpCredentialError".into(),
                message: "Failed to get GCP credentials".into(),
            }
        );
    }
}
