use anyhow::Result;
use baml_types::{BamlValue, EvaluationContext, UnresolvedValue};
use indexmap::{IndexMap, IndexSet};
use internal_baml_core::ir::FieldType;
use std::{collections::HashMap, sync::Arc};
use thiserror::Error;

use crate::{internal::llm_client::llm_provider::LLMProvider, tracing::BamlTracer};

#[derive(Debug, Clone)]
pub struct SpanCtx {
    pub span_id: uuid::Uuid,
    pub name: String,
}

#[derive(Debug)]
pub struct PropertyAttributes {
    pub(crate) alias: Option<BamlValue>,
    pub(crate) skip: Option<bool>,
    pub(crate) meta: IndexMap<String, BamlValue>,
    pub(crate) constraints: Vec<baml_types::Constraint>,
    pub(crate) streaming_behavior: baml_types::StreamingBehavior,
}

#[derive(Debug)]
pub struct RuntimeEnumOverride {
    pub(crate) alias: Option<BamlValue>,
    pub(crate) values: IndexMap<String, PropertyAttributes>,
}

#[derive(Debug)]
pub struct RuntimeClassOverride {
    pub(crate) alias: Option<BamlValue>,
    pub(crate) new_fields: IndexMap<String, (FieldType, PropertyAttributes)>,
    pub(crate) update_fields: IndexMap<String, PropertyAttributes>,
}

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

impl Clone for AwsCredProviderImpl {
    fn clone(&self) -> Self {
        Self {
            req_tx: self.req_tx.clone(),
            resp_rx: self.resp_rx.resubscribe(),
        }
    }
}

cfg_if::cfg_if!(
    if #[cfg(target_arch = "wasm32")] {
        use core::pin::Pin;
        use core::future::Future;
        pub type BamlSrcReader = Option<Box<dyn Fn(&str) -> core::pin::Pin<Box<dyn Future<Output = Result<Vec<u8>>>>>>>;
    } else {
        use futures::future::BoxFuture;
        pub type BamlSrcReader = Option<Box<fn(&str) -> BoxFuture<'static, Result<Vec<u8>>>>>;
    }
);

// #[derive(Debug)]
pub struct RuntimeContext {
    // path to baml_src in the local filesystem
    pub baml_src: Arc<BamlSrcReader>,
    pub aws_cred_provider: AwsCredProvider,
    env: HashMap<String, String>,
    pub tags: HashMap<String, BamlValue>,
    pub client_overrides: Option<(Option<String>, HashMap<String, Arc<LLMProvider>>)>,
    pub class_override: IndexMap<String, RuntimeClassOverride>,
    pub enum_overrides: IndexMap<String, RuntimeEnumOverride>,
    pub type_alias_overrides: IndexMap<String, FieldType>,
    pub recursive_type_alias_overrides: Vec<IndexMap<String, FieldType>>,
    // Only the BAML_TRACER depends on this. If it goes missing due to a bug, we just wont publish logs.
    pub span_id: Option<uuid::Uuid>,
    pub recursive_class_overrides: Vec<IndexSet<String>>,
}

impl RuntimeContext {
    pub fn eval_ctx(&self, strict: bool) -> EvaluationContext<'_> {
        EvaluationContext::new(&self.env, !strict)
    }

    pub fn env_vars(&self) -> &HashMap<String, String> {
        &self.env
    }

    pub fn proxy_url(&self) -> Option<&str> {
        self.env.get("BOUNDARY_PROXY_URL").map(|s| s.as_str())
    }

    pub fn new(
        baml_src: Arc<BamlSrcReader>,
        aws_cred_provider: AwsCredProvider,
        env: HashMap<String, String>,
        tags: HashMap<String, BamlValue>,
        client_overrides: Option<(Option<String>, HashMap<String, Arc<LLMProvider>>)>,
        class_override: IndexMap<String, RuntimeClassOverride>,
        enum_overrides: IndexMap<String, RuntimeEnumOverride>,
        type_alias_overrides: IndexMap<String, FieldType>,
        recursive_class_overrides: Vec<IndexSet<String>>,
        recursive_type_alias_overrides: Vec<IndexMap<String, FieldType>>,
        span_id: Option<uuid::Uuid>,
    ) -> RuntimeContext {
        RuntimeContext {
            baml_src,
            aws_cred_provider,
            env,
            tags,
            client_overrides,
            class_override,
            enum_overrides,
            type_alias_overrides,
            recursive_type_alias_overrides,
            span_id,
            recursive_class_overrides,
        }
    }

    pub fn resolve_expression<T: serde::de::DeserializeOwned>(
        &self,
        expr: &UnresolvedValue<()>,
        // If true, will return an error if any environment variables are not set
        // otherwise, will return a value with the missing environment variables replaced with the string "${key}"
        strict: bool,
    ) -> Result<T> {
        let ctx = EvaluationContext::new(&self.env, strict);
        match expr.resolve_serde::<T>(&ctx) {
            Ok(v) => Ok(v),
            Err(e) => anyhow::bail!(
                "Failed to resolve expression {:?} with error: {:?}",
                expr,
                e
            ),
        }
    }
}
