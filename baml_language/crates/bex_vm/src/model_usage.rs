//! Model usage from `ai.Client.invoke` implementations (and a stream's settle
//! step): when one returns, the usage of the calls its turn made itself is
//! charged to the innermost span. Read natively, so it needs no value capture
//! and works whether or not the invoke is a span.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use bex_vm_types::{Value, ValueKind};
pub use btel_records::ModelUsage;

use crate::{BexVm, telemetry::merge_usage};

/// Opt-in usage summary shared by one invocation and its child tasks.
#[derive(Clone, Debug)]
pub struct ModelUsageCollector(Arc<Mutex<Option<ModelUsageState>>>);

#[derive(Debug, Default)]
struct ModelUsageState {
    active_executions: usize,
    entries: Vec<ModelUsage>,
}

impl Default for ModelUsageCollector {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(Some(ModelUsageState::default()))))
    }
}

impl ModelUsageCollector {
    fn lock(&self) -> MutexGuard<'_, Option<ModelUsageState>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn start_execution(&self) -> Option<ModelUsageCapture> {
        self.lock().as_mut()?.active_executions += 1;
        Some(ModelUsageCapture(self.clone()))
    }

    /// Close collection. Only return a total if every execution has finished.
    pub fn finish(&self) -> Option<Vec<ModelUsage>> {
        let state = self.lock().take()?;
        if state.active_executions != 0 {
            return None;
        }
        let mut entries = state.entries;
        entries.sort_by(|a, b| (&a.provider, &a.model).cmp(&(&b.provider, &b.model)));
        Some(entries)
    }
}

/// Counts a running VM independently of retained invocation context handles.
pub(crate) struct ModelUsageCapture(ModelUsageCollector);

impl ModelUsageCapture {
    fn record(&self, entries: Vec<ModelUsage>) {
        if let Some(state) = self.0.lock().as_mut() {
            merge_usage(&mut state.entries, entries);
        }
    }
}

impl Drop for ModelUsageCapture {
    fn drop(&mut self) {
        if let Some(state) = self.0.lock().as_mut() {
            state.active_executions -= 1;
        }
    }
}

impl BexVm {
    pub fn model_usage_collector(&self) -> Option<&ModelUsageCollector> {
        self.model_usage_capture.as_ref().map(|capture| &capture.0)
    }

    pub fn set_model_usage_collector(&mut self, collector: Option<ModelUsageCollector>) {
        self.model_usage_capture = collector.and_then(|collector| collector.start_execution());
    }

    /// Stop collecting before a settled future wakes its caller.
    pub fn finish_model_usage(&mut self) {
        self.model_usage_capture = None;
    }

    /// Charge the model usage of `turn`, an `ai.ModelTurn`, to the innermost
    /// span. A value of another shape charges nothing.
    #[cold]
    pub(crate) fn charge_model_usage(&mut self, turn: Value) {
        if self.telemetry.is_none() && self.model_usage_capture.is_none() {
            return;
        }
        let entries = self.turn_usage(turn);
        if entries.is_empty() {
            return;
        }
        if let Some(capture) = &self.model_usage_capture {
            if self.telemetry.is_none() {
                capture.record(entries);
                return;
            }
            capture.record(entries.clone());
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
