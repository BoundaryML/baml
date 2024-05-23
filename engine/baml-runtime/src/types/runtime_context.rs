use anyhow::Result;
use baml_types::BamlValue;
use internal_baml_core::ir::repr::Expression;
use serde::{self, Deserialize};
use serde_json::{self};
use std::collections::HashMap;

#[derive(Deserialize, Debug, Default, Clone)]
pub struct SpanCtx {
    pub span_id: uuid::Uuid,
    pub name: String,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct RuntimeContext {
    #[serde(default = "HashMap::new")]
    pub env: HashMap<String, String>,
    #[serde(default = "HashMap::new")]
    pub tags: HashMap<String, BamlValue>,
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
