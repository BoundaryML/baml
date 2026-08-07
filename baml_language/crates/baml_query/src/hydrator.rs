use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Map, Value};

use crate::{QueryContext, QueryError, Result, ValueId, ValueStore};

pub type HydratedValue = Value;

/// A value file may contain this object to refer to another content-addressed
/// value. The marker is deliberately explicit so ordinary JSON strings are not
/// mistaken for references.
pub(crate) const VALUE_REFERENCE_KEY: &str = "$value_ref";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValueReference(pub ValueId);

#[async_trait]
pub trait Hydrator: Send + Sync {
    async fn hydrate_many(
        &self,
        roots: &[ValueId],
        context: &QueryContext,
    ) -> Result<HashMap<ValueId, HydratedValue>>;
}

pub struct RecursiveHydrator {
    store: Arc<dyn ValueStore>,
}

impl RecursiveHydrator {
    pub fn new(store: Arc<dyn ValueStore>) -> Self {
        Self { store }
    }

    async fn load_graph(
        &self,
        roots: &[ValueId],
        context: &QueryContext,
    ) -> Result<HashMap<ValueId, Value>> {
        let mut pending: Vec<ValueId> = roots.to_vec();
        let mut loaded = HashMap::new();
        let mut seen = HashSet::new();

        while !pending.is_empty() {
            context.check_cancelled()?;
            let mut request = Vec::new();
            for id in pending.drain(..) {
                if seen.insert(id) {
                    if let Some(value) = context.cached_value(&id) {
                        context
                            .metrics
                            .cache_hits
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        loaded.insert(id, value);
                    } else {
                        context
                            .metrics
                            .cache_misses
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        request.push(id);
                    }
                }
            }
            if request.len() + loaded.len() > context.budgets.max_distinct_values {
                return Err(QueryError::ValueLimit);
            }
            let blobs = self.store.get_many(&request, context).await?;
            for id in request {
                let bytes = blobs.get(&id).ok_or_else(|| QueryError::MissingValue {
                    value_id: id.to_string(),
                    path: std::path::PathBuf::new(),
                })?;
                let value: Value =
                    serde_json::from_slice(bytes).map_err(|error| QueryError::CorruptValue {
                        value_id: id.to_string(),
                        message: error.to_string(),
                    })?;
                if ValueId::from_content(bytes) != id {
                    return Err(QueryError::CorruptValue {
                        value_id: id.to_string(),
                        message: "content hash does not match the requested ID".to_owned(),
                    });
                }
                collect_references(&value, &mut pending)?;
                loaded.insert(id, value);
            }
        }
        Ok(loaded)
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve(
        &self,
        id: ValueId,
        value: &Value,
        graph: &HashMap<ValueId, Value>,
        active: &mut HashSet<ValueId>,
        depth: usize,
        expanded_bytes: &mut usize,
        context: &QueryContext,
    ) -> Result<Value> {
        if depth > context.budgets.max_value_depth {
            return Err(QueryError::ValueLimit);
        }
        if !active.insert(id) {
            return Err(QueryError::ValueCycle(id.to_string()));
        }
        let result = self.resolve_value(value, graph, active, depth, expanded_bytes, context);
        active.remove(&id);
        result
    }

    fn resolve_value(
        &self,
        value: &Value,
        graph: &HashMap<ValueId, Value>,
        active: &mut HashSet<ValueId>,
        depth: usize,
        expanded_bytes: &mut usize,
        context: &QueryContext,
    ) -> Result<Value> {
        let result = match value {
            Value::Object(object)
                if object.len() == 1 && object.contains_key(VALUE_REFERENCE_KEY) =>
            {
                let reference = object
                    .get(VALUE_REFERENCE_KEY)
                    .and_then(Value::as_str)
                    .ok_or_else(|| QueryError::CorruptValue {
                        value_id: "inline".to_owned(),
                        message: "reference must be a string".to_owned(),
                    })?;
                let id = ValueId::from_hex(reference)?;
                let child = graph.get(&id).ok_or_else(|| QueryError::MissingValue {
                    value_id: id.to_string(),
                    path: std::path::PathBuf::new(),
                })?;
                self.resolve(id, child, graph, active, depth + 1, expanded_bytes, context)?
            }
            Value::Array(values) => Value::Array(
                values
                    .iter()
                    .map(|value| {
                        self.resolve_value(value, graph, active, depth + 1, expanded_bytes, context)
                    })
                    .collect::<Result<_>>()?,
            ),
            Value::Object(object) => {
                let mut output = Map::new();
                for (key, value) in object {
                    output.insert(
                        key.clone(),
                        self.resolve_value(
                            value,
                            graph,
                            active,
                            depth + 1,
                            expanded_bytes,
                            context,
                        )?,
                    );
                }
                Value::Object(output)
            }
            primitive => primitive.clone(),
        };
        *expanded_bytes += serde_json::to_vec(&result)?.len();
        if *expanded_bytes > context.budgets.max_expanded_bytes {
            return Err(QueryError::ValueLimit);
        }
        Ok(result)
    }
}

#[async_trait]
impl Hydrator for RecursiveHydrator {
    async fn hydrate_many(
        &self,
        roots: &[ValueId],
        context: &QueryContext,
    ) -> Result<HashMap<ValueId, HydratedValue>> {
        let graph = self.load_graph(roots, context).await?;
        context.metrics.distinct_root_ids.fetch_add(
            roots.iter().copied().collect::<HashSet<_>>().len(),
            std::sync::atomic::Ordering::Relaxed,
        );
        let mut resolved_graph = HashMap::with_capacity(graph.len());
        for (id, value) in &graph {
            let mut expanded_bytes = 0;
            let resolved = self.resolve(
                *id,
                value,
                &graph,
                &mut HashSet::new(),
                0,
                &mut expanded_bytes,
                context,
            )?;
            context.cache_value(*id, resolved.clone());
            resolved_graph.insert(*id, resolved);
        }
        let mut output = HashMap::new();
        for id in roots.iter().copied() {
            let value = resolved_graph
                .get(&id)
                .ok_or_else(|| QueryError::MissingValue {
                    value_id: id.to_string(),
                    path: std::path::PathBuf::new(),
                })?
                .clone();
            output.insert(id, value);
        }
        Ok(output)
    }
}

fn collect_references(value: &Value, output: &mut Vec<ValueId>) -> Result<()> {
    match value {
        Value::Object(object) if object.len() == 1 && object.contains_key(VALUE_REFERENCE_KEY) => {
            let value = object
                .get(VALUE_REFERENCE_KEY)
                .and_then(Value::as_str)
                .ok_or_else(|| QueryError::CorruptValue {
                    value_id: "inline".to_owned(),
                    message: "reference must be a string".to_owned(),
                })?;
            output.push(ValueId::from_hex(value)?);
        }
        Value::Array(values) => {
            for value in values {
                collect_references(value, output)?;
            }
        }
        Value::Object(object) => {
            for value in object.values() {
                collect_references(value, output)?;
            }
        }
        _ => {}
    }
    Ok(())
}
