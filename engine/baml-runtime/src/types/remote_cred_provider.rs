use thiserror::Error;

#[derive(Debug, Error, Clone)]
/// For baml-src-reader and aws-cred-provider, provide a statically defined type which is Send + Sync
/// anyhow::Error is not Send + Sync, so it's convoluted to use it in this callback context
pub enum RuntimeCallbackError {
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

pub struct AwsCredProviderImpl {
    pub req_tx: tokio::sync::mpsc::Sender<Option<String>>,
    pub resp_rx: tokio::sync::broadcast::Receiver<RuntimeCallbackResult<AwsCredResult>>,
}

impl AwsCredProviderImpl {}

impl Clone for AwsCredProviderImpl {
    fn clone(&self) -> Self {
        Self {
            req_tx: self.req_tx.clone(),
            resp_rx: self.resp_rx.resubscribe(),
        }
    }
}
