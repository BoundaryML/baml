use anyhow::Result;
use baml_types::BamlValue;
use internal_baml_core::ir::{repr::Expression, FieldType};
use serde;
use serde_json;
use std::collections::HashMap;

#[derive(Debug)]
pub struct SpanCtx {
    pub span_id: uuid::Uuid,
    pub name: String,
}

#[derive(Debug)]
pub struct PropertyAttributes {
    pub(crate) alias: Option<Option<String>>,
    pub(crate) skip: Option<bool>,
    pub(crate) meta: HashMap<String, BamlValue>,
}

#[derive(Debug)]
pub struct RuntimeEnumOverride {
    pub(crate) values: HashMap<String, PropertyAttributes>,
}

#[derive(Debug)]
pub struct RuntimeClassOverride {
    pub(crate) properties: HashMap<String, (FieldType, PropertyAttributes)>,
}

#[derive(Debug)]
pub struct RuntimeContext {
    pub env: HashMap<String, String>,
    pub tags: HashMap<String, BamlValue>,
    pub class_override: HashMap<String, RuntimeClassOverride>,
    pub enum_overrides: HashMap<String, RuntimeEnumOverride>,
}

impl RuntimeContext {
    pub fn resolve_expression<T: serde::de::DeserializeOwned>(
        &self,
        expr: &Expression,
    ) -> Result<T> {
        serde_json::from_value::<T>(super::expression_helper::to_value(self, expr)?).map_err(|e| {
            anyhow::anyhow!(
                "Failed to resolve expression {:?} with error: {:?}",
                expr,
                e
            )
        })
    }
}
