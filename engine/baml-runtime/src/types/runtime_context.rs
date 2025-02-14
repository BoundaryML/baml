use anyhow::Result;
use baml_types::{BamlValue, EvaluationContext, UnresolvedValue};
use indexmap::IndexMap;
use internal_baml_core::ir::FieldType;
use std::{collections::HashMap, sync::Arc};

use crate::internal::llm_client::llm_provider::LLMProvider;

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

// #[cfg(target_arch = "wasm32")]
// pub type BamlSrcReader = Box<dyn Fn(&str) -> Result<String>>;
// #[cfg(not(target_arch = "wasm32"))]
// pub type BamlSrcReader = fn(&str) -> Result<String>;
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
    env: HashMap<String, String>,
    pub tags: HashMap<String, BamlValue>,
    pub client_overrides: Option<(Option<String>, HashMap<String, Arc<LLMProvider>>)>,
    pub class_override: IndexMap<String, RuntimeClassOverride>,
    pub enum_overrides: IndexMap<String, RuntimeEnumOverride>,
    pub type_alias_overrides: IndexMap<String, FieldType>,
    pub recursive_type_alias_overrides: Vec<IndexMap<String, FieldType>>,
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
        env: HashMap<String, String>,
        tags: HashMap<String, BamlValue>,
        client_overrides: Option<(Option<String>, HashMap<String, Arc<LLMProvider>>)>,
        class_override: IndexMap<String, RuntimeClassOverride>,
        enum_overrides: IndexMap<String, RuntimeEnumOverride>,
        type_alias_overrides: IndexMap<String, FieldType>,
        recursive_type_alias_overrides: Vec<IndexMap<String, FieldType>>,
    ) -> RuntimeContext {
        RuntimeContext {
            baml_src,
            env,
            tags,
            client_overrides,
            class_override,
            enum_overrides,
            type_alias_overrides,
            recursive_type_alias_overrides,
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
