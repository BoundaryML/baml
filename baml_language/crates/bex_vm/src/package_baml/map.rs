use bex_heap::TlabHolder;
use bex_vm_types::{BexStr, types::Value};
use indexmap::IndexMap;

use super::{BamlClassMap, MapView, NativeCallResult, NativeFunctionResult, PackageBamlImpl};
use crate::BexVm;

// ─── BamlClassMap trait implementation ───────────────────────────────────────

impl BamlClassMap for PackageBamlImpl {
    #[allow(clippy::cast_possible_wrap)]
    fn length(map: MapView<'_>) -> i64 {
        map.len() as i64
    }

    fn has(vm: &BexVm, map: MapView<'_>, key: &Value) -> bool {
        if let Ok(k) = vm.as_string(key) {
            map.contains_key(k.as_str())
        } else {
            false
        }
    }

    fn keys(vm: &mut BexVm, map: MapView<'_>) -> Vec<Value> {
        map.keys()
            .map(|k| Value::object(vm.alloc_string(k.clone())))
            .collect()
    }

    fn values(map: MapView<'_>) -> Vec<Value> {
        map.values().copied().collect()
    }

    // ── set ──────────────────────────────────────────────────────────────────
    //
    // `set` (and the other mut-self methods below) need a custom glue so the
    // immutable VM borrow used to extract the string key is dropped before the
    // mutable `as_map_mut` borrow. The trait method bodies are unreachable —
    // the glue dispatches to the actual logic.

    fn __glue_set(vm: &mut BexVm, args: &[Value]) -> NativeCallResult {
        let result: NativeFunctionResult = (|| {
            let key_as_string = vm.as_string(&args[1])?.clone();
            let value = args[2];
            let mut map = vm.as_map_mut(&args[0])?;
            // `IndexMap::insert` returns `Some(prev)` if the key already
            // existed, `None` otherwise — exactly the V? semantics we want.
            Ok(match map.insert(key_as_string, value) {
                Some(prev) => prev,
                None => Value::NULL,
            })
        })();
        match result {
            Ok(v) => NativeCallResult::Done(v),
            Err(e) => NativeCallResult::Error(e),
        }
    }

    fn set(_map: &mut IndexMap<BexStr, Value>, _key: &Value, _value: &Value) -> Option<Value> {
        unreachable!("Map.set: should be dispatched via __glue_set")
    }

    fn get(vm: &BexVm, map: MapView<'_>, key: &Value) -> Option<Value> {
        if let Ok(k) = vm.as_string(key) {
            map.get(k.as_str()).copied()
        } else {
            None
        }
    }

    // ── delete ────────────────────────────────────────────────────────────────

    fn __glue_delete(vm: &mut BexVm, args: &[Value]) -> NativeCallResult {
        let result: NativeFunctionResult = (|| {
            let key_as_string = vm.as_string(&args[1])?.clone();
            let mut map = vm.as_map_mut(&args[0])?;
            // `shift_remove` preserves the order of remaining entries (matching
            // insertion order) — important since `keys()` / `values()` return
            // entries in insertion order.
            Ok(match map.shift_remove(key_as_string.as_str()) {
                Some(prev) => prev,
                None => Value::NULL,
            })
        })();
        match result {
            Ok(v) => NativeCallResult::Done(v),
            Err(e) => NativeCallResult::Error(e),
        }
    }

    fn delete(_map: &mut IndexMap<BexStr, Value>, _key: &Value) -> Option<Value> {
        unreachable!("Map.delete: should be dispatched via __glue_delete")
    }

    // ── get_or_insert ─────────────────────────────────────────────────────────

    fn __glue_get_or_insert(vm: &mut BexVm, args: &[Value]) -> NativeCallResult {
        let result: NativeFunctionResult = (|| {
            let key_as_string = vm.as_string(&args[1])?.clone();
            let default = args[2];
            let mut map = vm.as_map_mut(&args[0])?;
            Ok(*map.entry(key_as_string).or_insert(default))
        })();
        match result {
            Ok(v) => NativeCallResult::Done(v),
            Err(e) => NativeCallResult::Error(e),
        }
    }

    fn get_or_insert(_map: &mut IndexMap<BexStr, Value>, _key: &Value, _default: &Value) -> Value {
        unreachable!("Map.get_or_insert: should be dispatched via __glue_get_or_insert")
    }

    // ── clear ─────────────────────────────────────────────────────────────────

    #[allow(clippy::unused_unit)]
    fn clear(map: &mut IndexMap<BexStr, Value>) -> () {
        map.clear();
    }
}
