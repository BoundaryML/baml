//! Model usage from `ai.Client.invoke` implementations (and a stream's settle
//! step): when one returns, the usage of the calls its turn made itself is
//! charged to the innermost span. Read natively, so it needs no value capture
//! and works whether or not the invoke is a span.

use std::sync::{Arc, Mutex, PoisonError};

use bex_vm_types::{Value, ValueKind};
pub use btel_records::ModelUsage;

use crate::{BexVm, telemetry::merge_usage};

/// Opt-in usage summary shared by one invocation and its child tasks.
#[derive(Clone, Debug, Default)]
pub struct ModelUsageCollector(Arc<Mutex<Vec<ModelUsage>>>);

impl ModelUsageCollector {
    /// Snapshot the usage reported up to the time of this call.
    pub fn snapshot(&self) -> Vec<ModelUsage> {
        let mut entries = self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        entries.sort_by(|a, b| (&a.provider, &a.model).cmp(&(&b.provider, &b.model)));
        entries
    }

    fn record(&self, entries: Vec<ModelUsage>) {
        merge_usage(
            &mut self.0.lock().unwrap_or_else(PoisonError::into_inner),
            entries,
        );
    }
}

impl BexVm {
    pub fn model_usage_collector(&self) -> Option<&ModelUsageCollector> {
        self.model_usage_collector.as_ref()
    }

    pub fn set_model_usage_collector(&mut self, collector: Option<ModelUsageCollector>) {
        self.model_usage_collector = collector;
    }

    /// Charge the model usage of `turn`, an `ai.ModelTurn`, to the innermost
    /// span. A value of another shape charges nothing.
    #[cold]
    pub(crate) fn charge_model_usage(&mut self, turn: Value) {
        if self.telemetry.is_none() && self.model_usage_collector.is_none() {
            return;
        }
        let entries = self.turn_usage(turn);
        if entries.is_empty() {
            return;
        }
        if let Some(collector) = &self.model_usage_collector {
            if self.telemetry.is_none() {
                collector.record(entries);
                return;
            }
            collector.record(entries.clone());
        }
        if let Some(telemetry) = &mut self.telemetry {
            telemetry.charge_model_usage(entries);
        }
    }

    /// One entry per call in `turn.calls` that reported usage. Child turns are
    /// not read: each was charged when its own invoke returned.
    fn turn_usage(&self, turn: Value) -> Vec<ModelUsage> {
        let Some(calls) = self.field(turn, "calls") else {
            return Vec::new();
        };
        let Ok(calls) = self.as_array(&calls) else {
            return Vec::new();
        };
        let calls: Vec<Value> = calls.iter().copied().collect();
        calls
            .into_iter()
            .filter_map(|call| {
                let usage = self.field(call, "usage").filter(|usage| !usage.is_null())?;
                let count = |name: &str| {
                    self.field(usage, name)
                        .and_then(|value| match value.kind() {
                            ValueKind::Int(n) => u64::try_from(n).ok(),
                            _ => None,
                        })
                };
                let key = self.field(call, "resolved_model")?;
                let text = |name: &str| {
                    self.field(key, name).and_then(|value| {
                        self.as_string(&value).ok().map(|s| Box::from(s.as_str()))
                    })
                };
                Some(ModelUsage {
                    provider: text("provider")?,
                    model: text("model"),
                    calls: 1,
                    input_tokens: count("uncached_input_tokens").or_else(|| count("input_tokens")),
                    output_tokens: count("output_tokens"),
                    cache_read_tokens: count("cached_input_tokens"),
                    cache_write_tokens: count("cache_write_input_tokens"),
                    reasoning_tokens: count("reasoning_tokens"),
                })
            })
            .collect()
    }

    fn field(&self, value: Value, name: &str) -> Option<Value> {
        let instance = self.as_instance(&value).ok()?;
        let class = instance.class_object();
        let index = class
            .instance_fields()
            .iter()
            .position(|field| field.name == name)?;
        Some(instance.load_value(index))
    }
}
