use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use baml_ids::FunctionCallId;
use baml_types::{BamlValue, EvaluationContext, UnresolvedValue};
use indexmap::{IndexMap, IndexSet};
use internal_baml_core::ir::TypeIR;
use thiserror::Error;

use crate::{internal::llm_client::llm_provider::LLMProvider, tracing::BamlTracer};

#[derive(Debug, Clone)]
pub struct CallCtx {
    pub call_id: uuid::Uuid,
    pub name: String,
    pub new_call_id: FunctionCallId,
}

#[derive(Debug)]
pub struct PropertyAttributes {
    pub(crate) alias: Option<BamlValue>,
    pub(crate) skip: Option<bool>,
    pub(crate) meta: IndexMap<String, BamlValue>,
    pub(crate) constraints: Vec<baml_types::Constraint>,
    pub(crate) streaming_behavior: baml_types::type_meta::base::StreamingBehavior,
}

#[derive(Debug)]
pub struct RuntimeEnumOverride {
    pub(crate) alias: Option<BamlValue>,
    pub(crate) values: IndexMap<String, PropertyAttributes>,
}

#[derive(Debug)]
pub struct RuntimeClassOverride {
    pub(crate) alias: Option<BamlValue>,
    pub(crate) new_fields: IndexMap<String, (TypeIR, PropertyAttributes)>,
    pub(crate) update_fields: IndexMap<String, PropertyAttributes>,
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
    env: HashMap<String, String>,
    pub tags: HashMap<String, BamlValue>,
    pub client_overrides: Option<(Option<String>, HashMap<String, Arc<LLMProvider>>)>,
    pub class_override: IndexMap<String, RuntimeClassOverride>,
    pub enum_overrides: IndexMap<String, RuntimeEnumOverride>,
    pub type_alias_overrides: IndexMap<String, TypeIR>,
    pub recursive_type_alias_overrides: Vec<IndexMap<String, TypeIR>>,
    // Only the BAML_TRACER depends on this.
    pub call_id_stack: Vec<FunctionCallId>,
    pub recursive_class_overrides: Vec<IndexSet<String>>,
    /// Called through modular API.
    is_modular_api: bool,
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

    pub fn is_modular_api(&self) -> bool {
        self.is_modular_api
    }

    pub fn set_modular_api(&mut self, is_modular_api: bool) {
        self.is_modular_api = is_modular_api;
    }

    pub fn new(
        baml_src: Arc<BamlSrcReader>,
        env: HashMap<String, String>,
        tags: HashMap<String, BamlValue>,
        client_overrides: Option<(Option<String>, HashMap<String, Arc<LLMProvider>>)>,
        class_override: IndexMap<String, RuntimeClassOverride>,
        enum_overrides: IndexMap<String, RuntimeEnumOverride>,
        type_alias_overrides: IndexMap<String, TypeIR>,
        recursive_class_overrides: Vec<IndexSet<String>>,
        recursive_type_alias_overrides: Vec<IndexMap<String, TypeIR>>,
        call_id_stack: Vec<FunctionCallId>,
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
            call_id_stack,
            recursive_class_overrides,
            is_modular_api: false,
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
