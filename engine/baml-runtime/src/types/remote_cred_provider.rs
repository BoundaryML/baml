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

static REMOTE_CRED_PROVIDER_SINGLETON: OnceLock<AwsCredProviderImpl> = OnceLock::new();

pub fn get_remote_cred_provider() -> Result<&'static AwsCredProviderImpl, RuntimeCallbackError> {
    REMOTE_CRED_PROVIDER_SINGLETON
        .get()
        .ok_or(RuntimeCallbackError::NoCredProviderBridge)
}

pub fn set_remote_cred_provider(aws_cred_provider: AwsCredProviderImpl) {
    REMOTE_CRED_PROVIDER_SINGLETON.set(aws_cred_provider);
}

#[derive(serde::Deserialize, Debug, Clone)]
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

#[derive(serde::Deserialize, Debug, Clone)]
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
pub struct AwsCredProviderImpl {
    aws_req_tx: tokio::sync::mpsc::Sender<Option<String>>,
    aws_resp_rx: tokio::sync::broadcast::Receiver<RuntimeCallbackResult<AwsCredResult>>,
    gcp_req_tx: tokio::sync::mpsc::Sender<Option<String>>,
    gcp_resp_rx: tokio::sync::broadcast::Receiver<RuntimeCallbackResult<GcpCredResult>>,
}

impl AwsCredProviderImpl {
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

impl Clone for AwsCredProviderImpl {
    fn clone(&self) -> Self {
        Self {
            aws_req_tx: self.aws_req_tx.clone(),
            aws_resp_rx: self.aws_resp_rx.resubscribe(),
            gcp_req_tx: self.gcp_req_tx.clone(),
            gcp_resp_rx: self.gcp_resp_rx.resubscribe(),
        }
    }
}
