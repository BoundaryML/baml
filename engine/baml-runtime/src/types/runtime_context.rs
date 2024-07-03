use anyhow::Result;
use baml_types::BamlValue;
use indexmap::IndexMap;
use internal_baml_core::ir::{repr::Expression, FieldType};
use serde;
use serde_json;
use std::{collections::HashMap, sync::Arc};

use crate::internal::llm_client::llm_provider::LLMProvider;

#[derive(Debug)]
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

#[derive(Debug)]
pub struct RuntimeContext {
    pub env: HashMap<String, String>,
    pub tags: HashMap<String, BamlValue>,
    pub client_overrides: Option<(Option<String>, HashMap<String, Arc<LLMProvider>>)>,
    pub class_override: IndexMap<String, RuntimeClassOverride>,
    pub enum_overrides: IndexMap<String, RuntimeEnumOverride>,
}

impl RuntimeContext {
    pub fn resolve_expression<T: serde::de::DeserializeOwned>(
        &self,
        expr: &Expression,
    ) -> Result<T> {
        match super::expression_helper::to_value(self, expr) {
            Ok(v) => serde_json::from_value(v).map_err(|e| e.into()),
            Err(e) => Err(e),
        }
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to resolve expression {:?} with error: {:?}",
                expr,
                e
            )
        })
    }
}
