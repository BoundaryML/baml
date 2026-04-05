use bex_vm_types::types::Value;
use indexmap::IndexMap;

use super::{BamlClassMap, PackageBamlImpl};
use crate::BexVm;

impl BamlClassMap for PackageBamlImpl {
    #[allow(clippy::cast_possible_wrap)]
    fn length(map: &IndexMap<String, Value>) -> i64 {
        map.len() as i64
    }

    fn has(vm: &mut BexVm, map: &IndexMap<String, Value>, key: &Value) -> bool {
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

    fn __glue_set(vm: &mut BexVm, args: &[Value]) -> super::NativeFunctionResult {
        let key = &args[1];
        let value = &args[2];
        let key_as_string = vm.as_string(key)?.clone();
        let map = vm.as_map_mut(&args[0])?;
        map.insert(key_as_string, *value);
        Ok(Value::Null)
    }

    fn set(_map: &mut IndexMap<String, Value>, _key: &Value, _value: &Value) {
        panic!("Should be called via __glue_set");
    }

    fn get(vm: &mut BexVm, map: &IndexMap<String, Value>, key: &Value) -> Option<Value> {
        if let Ok(k) = vm.as_string(key) {
            map.get(k.as_str()).copied()
        } else {
            None
        }
    }

    fn map(_map: &IndexMap<String, Value>, _f: &Value) -> Vec<Value> {
        todo!("map not yet implemented");
    }
}
