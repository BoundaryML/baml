//! Arguments given as JSON text, hydrated by the stdlib.
//!
//! A host that holds a call's arguments as JSON (the playground's arguments
//! form, the CLI's `--json-args`) hands the text to the engine together with
//! the declared parameter type, and `baml.json.deserialize<T>` builds the
//! value. The host converts nothing, and a user's `from_json` override applies
//! exactly as it would inside BAML.

use std::{collections::HashMap, sync::Arc};

use indexmap::IndexMap;

use crate::{BexEngine, BexExternalValue, EngineError, FunctionCallContext, RuntimeTy};

/// Why a JSON object could not become a call's arguments. Boxed by the
/// functions that return it: an engine error inside is large, and the happy
/// path should not pay for it.
#[derive(Debug, thiserror::Error)]
pub enum JsonArgumentsError {
    #[error("arguments are not valid JSON: {0}")]
    Syntax(serde_json::Error),
    #[error("arguments must be a JSON object keyed by parameter name")]
    NotAnObject,
    #[error("missing required argument `{name}` (type: {ty})")]
    Missing { name: String, ty: RuntimeTy },
    #[error("unknown argument(s): {}", .0.join(", "))]
    Unknown(Vec<String>),
    #[error("argument `{name}`: {source}")]
    Parameter {
        name: String,
        #[source]
        source: EngineError,
    },
    #[error(transparent)]
    Engine(EngineError),
}

impl BexEngine {
    /// Hydrate one JSON document into a value of `ty` through
    /// `baml.json.deserialize<T>`. `call_ctx` builds the helper call's context
    /// from the type binding this supplies, so the caller keeps its logger and
    /// cancellation.
    pub async fn deserialize_json(
        self: &Arc<Self>,
        json_text: &str,
        ty: &RuntimeTy,
        call_ctx: impl FnOnce(IndexMap<String, RuntimeTy>) -> FunctionCallContext,
    ) -> Result<BexExternalValue, EngineError> {
        self.call_function(
            "baml.json.deserialize",
            vec![BexExternalValue::String(json_text.into())],
            call_ctx(IndexMap::from([("T".to_string(), ty.clone())])),
            true,
        )
        .await
    }

    /// The arguments of a call to `function_name`, hydrated from one JSON
    /// object keyed by parameter name. Every field is deserialized against its
    /// parameter's declared type; a parameter with a default may be absent;
    /// anything else missing, and any field no parameter claims, is an error.
    pub async fn json_arguments(
        self: &Arc<Self>,
        function_name: &str,
        json: &str,
        call_ctx: impl Fn(IndexMap<String, RuntimeTy>) -> FunctionCallContext,
    ) -> Result<HashMap<String, BexExternalValue>, Box<JsonArgumentsError>> {
        let parsed =
            serde_json::from_str(json).map_err(|e| Box::new(JsonArgumentsError::Syntax(e)))?;
        let serde_json::Value::Object(mut fields) = parsed else {
            return Err(Box::new(JsonArgumentsError::NotAnObject));
        };
        let params: Vec<(String, RuntimeTy, bool)> = self
            .function_params(function_name)
            .map_err(|e| Box::new(JsonArgumentsError::Engine(e)))?
            .into_iter()
            .map(|(name, ty, has_default)| (name.to_string(), ty, has_default))
            .collect();
        let mut arguments = HashMap::with_capacity(params.len());
        for (name, ty, has_default) in params {
            match fields.remove(&name) {
                Some(value) => {
                    let value = self
                        .deserialize_json(&value.to_string(), &ty, &call_ctx)
                        .await
                        .map_err(|source| {
                            Box::new(JsonArgumentsError::Parameter {
                                name: name.clone(),
                                source,
                            })
                        })?;
                    arguments.insert(name, value);
                }
                None if has_default => {}
                None => return Err(Box::new(JsonArgumentsError::Missing { name, ty })),
            }
        }
        if !fields.is_empty() {
            return Err(Box::new(JsonArgumentsError::Unknown(
                fields.into_iter().map(|(name, _)| name).collect(),
            )));
        }
        Ok(arguments)
    }
}
