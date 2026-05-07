use std::collections::HashMap;

use bex_vm_types::{HeapPtr, types::Value};
use indexmap::IndexMap;

use super::{BamlClassMap, Continuation, NativeCallResult, PackageBamlImpl, make_to_json_callee};
use crate::BexVm;

// ─── Map.to_json continuation ────────────────────────────────────────────────

/// Continuation for `Map.to_json`. Dispatches `v.to_json()` for each map entry
/// in order, accumulating `(key, json_value)` pairs and finalizing into a
/// `map<string, json>` when the last entry is processed.
///
/// Because BAML maps are string-keyed at runtime regardless of the declared `K`
/// type, this handler can safely use each key as a plain `String`.
struct MapToJsonContinuation {
    /// All entries in the original map (key, value).
    entries: Vec<(String, Value)>,
    /// Index of the entry whose `to_json()` callback result we are about to receive.
    /// Starts at 0 (we yield for index 0 before constructing the continuation).
    idx: usize,
    /// Accumulated `(key, json_result)` pairs so far (does not include in-flight result).
    results: IndexMap<String, Value>,
}

impl Continuation for MapToJsonContinuation {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        // Store the result we just received.
        let key = self.entries[self.idx].0.clone();
        self.results.insert(key, value);
        self.idx += 1;

        if self.idx >= self.entries.len() {
            // All entries have been processed — return the completed map.
            return NativeCallResult::Done(vm.alloc_map(self.results));
        }

        // Yield to call to_json on the next value.
        let next_val = self.entries[self.idx].1;
        match make_to_json_callee(vm, next_val) {
            Ok(callee) => NativeCallResult::YieldToCall {
                callee,
                args: vec![],
                type_args: vec![],
                continuation: self,
            },
            Err(e) => NativeCallResult::Error(e),
        }
    }

    fn gc_roots(&self) -> Vec<HeapPtr> {
        let mut roots = Vec::new();
        for (_, v) in &self.entries {
            if let Value::Object(ptr) = v {
                roots.push(*ptr);
            }
        }
        for (_, v) in &self.results {
            if let Value::Object(ptr) = v {
                roots.push(*ptr);
            }
        }
        roots
    }

    fn apply_forwarding(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        for (_, v) in &mut self.entries {
            if let Value::Object(ptr) = v {
                if let Some(&new_ptr) = forwarding.get(ptr) {
                    *ptr = new_ptr;
                }
            }
        }
        for (_, v) in &mut self.results {
            if let Value::Object(ptr) = v {
                if let Some(&new_ptr) = forwarding.get(ptr) {
                    *ptr = new_ptr;
                }
            }
        }
    }
}

// ─── BamlClassMap trait implementation ───────────────────────────────────────

impl BamlClassMap for PackageBamlImpl {
    #[allow(clippy::cast_possible_wrap)]
    fn length(map: &IndexMap<String, Value>) -> i64 {
        map.len() as i64
    }

    fn has(vm: &BexVm, map: &IndexMap<String, Value>, key: &Value) -> bool {
        if let Ok(k) = vm.as_string(key) {
            map.contains_key(k.as_str())
        } else {
            false
        }
    }

    fn keys(vm: &mut BexVm, map: &IndexMap<String, Value>) -> Vec<Value> {
        map.keys().map(|k| vm.alloc_string(k.clone())).collect()
    }

    fn values(map: &IndexMap<String, Value>) -> Vec<Value> {
        map.values().copied().collect()
    }

    fn __glue_set(vm: &mut BexVm, args: &[Value]) -> super::NativeCallResult {
        let __result: super::NativeFunctionResult = (|| {
            let key = &args[1];
            let value = &args[2];
            let key_as_string = vm.as_string(key)?.clone();
            let map = vm.as_map_mut(&args[0])?;
            map.insert(key_as_string, *value);
            Ok(Value::Null)
        })();
        match __result {
            Ok(v) => super::NativeCallResult::Done(v),
            Err(e) => super::NativeCallResult::Error(e),
        }
    }

    fn set(_map: &mut IndexMap<String, Value>, _key: &Value, _value: &Value) {
        panic!("Should be called via __glue_set");
    }

    fn get(vm: &BexVm, map: &IndexMap<String, Value>, key: &Value) -> Option<Value> {
        if let Ok(k) = vm.as_string(key) {
            map.get(k.as_str()).copied()
        } else {
            None
        }
    }

    fn to_json(vm: &mut BexVm, map: &IndexMap<String, Value>) -> NativeCallResult {
        // Collect entries so we can iterate without holding a borrow on vm.
        let entries: Vec<(String, Value)> = map.iter().map(|(k, v)| (k.clone(), *v)).collect();

        if entries.is_empty() {
            return NativeCallResult::Done(vm.alloc_map(IndexMap::new()));
        }

        let first_val = entries[0].1;
        let callee = match make_to_json_callee(vm, first_val) {
            Ok(c) => c,
            Err(e) => return NativeCallResult::Error(e),
        };

        let capacity = entries.len();
        NativeCallResult::YieldToCall {
            callee,
            args: vec![],
            type_args: vec![],
            continuation: Box::new(MapToJsonContinuation {
                entries,
                idx: 0,
                results: IndexMap::with_capacity(capacity),
            }),
        }
    }
}
