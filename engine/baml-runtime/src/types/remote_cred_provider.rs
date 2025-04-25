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
}

static_assertions::assert_impl_all!(RuntimeCallbackError: Send, Sync);

pub type RuntimeCallbackResult<T> = Result<T, RuntimeCallbackError>;

pub type AwsCredProvider = Option<AwsCredProviderImpl>;

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

#[derive(new)]
pub struct AwsCredProviderImpl {
    req_tx: tokio::sync::mpsc::Sender<Option<String>>,
    resp_rx: tokio::sync::broadcast::Receiver<RuntimeCallbackResult<AwsCredResult>>,
}

impl AwsCredProviderImpl {
    pub async fn aws_req(
        &mut self,
        profile_name: Option<String>,
    ) -> RuntimeCallbackResult<AwsCredResult> {
        if let Err(e) = self.req_tx.send(profile_name).await {
            log::error!(
                "Failed to send AWS cred request across WASM bridge: {:?}",
                e
            );
            return Err(RuntimeCallbackError::SendError(e.to_string()));
        };
        let creds = match self.resp_rx.recv().await {
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

    pub async fn gcp_req(
        &mut self,
        profile_name: Option<String>,
    ) -> RuntimeCallbackResult<AwsCredResult> {
        if let Err(e) = self.req_tx.send(profile_name).await {
            log::error!(
                "Failed to send AWS cred request across WASM bridge: {:?}",
                e
            );
            return Err(RuntimeCallbackError::SendError(e.to_string()));
        };
        let creds = match self.resp_rx.recv().await {
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
}

impl Clone for AwsCredProviderImpl {
    fn clone(&self) -> Self {
        Self {
            req_tx: self.req_tx.clone(),
            resp_rx: self.resp_rx.resubscribe(),
        }
    }
}
