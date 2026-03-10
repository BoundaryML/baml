use bex_vm_types::types::Value;

use super::*;
use crate::BexVm;

impl BamlClassArray for PackageBamlImpl {
    #[allow(clippy::cast_possible_wrap)]
    fn length(array: &[Value]) -> i64 {
        array.len() as i64
    }

    fn push(array: &mut Vec<Value>, item: &Value) -> i64 {
        array.push(*item);
        array.len() as i64
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    fn at(array: &[Value], index: i64) -> Option<Value> {
        array.get(index as usize).copied()
    }

    fn concat(array: &[Value], other: &[Value]) -> Vec<Value> {
        array.iter().chain(other.iter()).copied().collect()
    }

    fn pop(array: &mut Vec<Value>) -> Option<Value> {
        array.pop()
    }

    fn reverse(array: &[Value]) -> Vec<Value> {
        let mut result = array.to_vec();
        result.reverse();
        result
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    fn slice(array: &[Value], start: i64, end: i64) -> Vec<Value> {
        let len = array.len() as i64;
        let start = start.max(0).min(len) as usize;
        let end = end.max(0).min(len) as usize;
        let end = end.max(start);
        array[start..end].to_vec()
    }

    fn join(vm: &mut BexVm, array: &[Value], separator: &str) -> String {
        array
            .iter()
            .map(|v| vm.as_string(v).map(|s| s.clone()).unwrap_or_default())
            .collect::<Vec<_>>()
            .join(separator)
    }
}
