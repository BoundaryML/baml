//! Model usage from `ai.Client.invoke` implementations (and a stream's settle
//! step): when one returns, the usage of the calls its turn made itself is
//! charged to the innermost span. Read natively, so it needs no value capture
//! and works whether or not the invoke is a span.

use bex_vm_types::{Object, Value, ValueKind};
use btel_records::ModelUsage;

use crate::BexVm;

impl BexVm {
    /// Charge the model usage of `turn`, an `ai.ModelTurn`, to the innermost
    /// span. A value of another shape charges nothing.
    #[cold]
    pub(crate) fn charge_model_usage(&mut self, turn: Value) {
        if self.telemetry.is_none() {
            return;
        }
        let entries = self.turn_usage(turn);
        if let Some(telemetry) = &mut self.telemetry
            && !entries.is_empty()
        {
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
                let model = self
                    .field(call, "resolved_model")
                    .and_then(|key| self.field(key, "model"))
                    .and_then(|model| self.as_string(&model).ok().map(|s| Box::from(s.as_str())));
                Some(ModelUsage {
                    model,
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
        let Object::Class(class) = self.get_object(instance.class) else {
            return None;
        };
        let index = class.fields.iter().position(|field| field.name == name)?;
        instance.try_load_field(index)
    }
}
