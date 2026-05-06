use std::collections::HashMap;

use bex_vm_types::{HeapPtr, types::Value};

use super::{BamlClassArray, Continuation, NativeCallResult, PackageBamlImpl, make_to_json_callee};
use crate::BexVm;

/// Continuation for `Array.map`. Accumulates the mapped results one element at
/// a time, yielding to the callback for each element and completing when the
/// last result has been collected.
struct MapContinuation {
    /// `HeapPtr` to the mapping function/closure (needed for GC).
    f_ptr: HeapPtr,
    /// The original array elements to map over.
    array: Vec<Value>,
    /// Index of the element whose callback result we are about to receive.
    /// Starts at 0 (we yield for index 0 before constructing the continuation).
    idx: usize,
    /// Accumulated mapped results so far (does not include the current in-flight result).
    results: Vec<Value>,
}

impl Continuation for MapContinuation {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        // Store the result we just received.
        self.results.push(value);
        self.idx += 1;

        if self.idx >= self.array.len() {
            // All elements have been mapped — return the completed array.
            return NativeCallResult::Done(vm.alloc_array(self.results));
        }

        // Yield to map the next element.
        let next_arg = self.array[self.idx];
        NativeCallResult::YieldToCall {
            callee: self.f_ptr,
            args: vec![next_arg],
            type_args: vec![],
            continuation: self,
        }
    }

    fn gc_roots(&self) -> Vec<HeapPtr> {
        // Collect all heap pointers captured by this continuation.
        let mut roots = vec![self.f_ptr];
        for v in &self.array {
            if let Value::Object(ptr) = v {
                roots.push(*ptr);
            }
        }
        for v in &self.results {
            if let Value::Object(ptr) = v {
                roots.push(*ptr);
            }
        }
        roots
    }

    fn apply_forwarding(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        if let Some(&new_ptr) = forwarding.get(&self.f_ptr) {
            self.f_ptr = new_ptr;
        }
        for v in &mut self.array {
            if let Value::Object(ptr) = v {
                if let Some(&new_ptr) = forwarding.get(ptr) {
                    *ptr = new_ptr;
                }
            }
        }
        for v in &mut self.results {
            if let Value::Object(ptr) = v {
                if let Some(&new_ptr) = forwarding.get(ptr) {
                    *ptr = new_ptr;
                }
            }
        }
    }
}

// ─── Array.to_json continuation ──────────────────────────────────────────────

/// Continuation for `Array.to_json`. Dispatches `v.to_json()` for each element
/// in order, accumulating json results and finalizing into a `json[]` value.
struct ToJsonContinuation {
    /// The original array elements.
    array: Vec<Value>,
    /// Index of the element whose `to_json()` callback result we are about to receive.
    /// Starts at 0 (we yield for index 0 before constructing the continuation).
    idx: usize,
    /// Accumulated json results so far (does not include in-flight result).
    results: Vec<Value>,
}

impl Continuation for ToJsonContinuation {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        self.results.push(value);
        self.idx += 1;

        if self.idx >= self.array.len() {
            return NativeCallResult::Done(vm.alloc_array(self.results));
        }

        let next_val = self.array[self.idx];
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
        for v in &self.array {
            if let Value::Object(ptr) = v {
                roots.push(*ptr);
            }
        }
        for v in &self.results {
            if let Value::Object(ptr) = v {
                roots.push(*ptr);
            }
        }
        roots
    }

    fn apply_forwarding(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        for v in &mut self.array {
            if let Value::Object(ptr) = v {
                if let Some(&new_ptr) = forwarding.get(ptr) {
                    *ptr = new_ptr;
                }
            }
        }
        for v in &mut self.results {
            if let Value::Object(ptr) = v {
                if let Some(&new_ptr) = forwarding.get(ptr) {
                    *ptr = new_ptr;
                }
            }
        }
    }
}

impl BamlClassArray for PackageBamlImpl {
    #[allow(clippy::cast_possible_wrap)]
    fn length(array: &[Value]) -> i64 {
        array.len() as i64
    }

    #[allow(clippy::cast_possible_wrap)]
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

    #[allow(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap
    )]
    fn slice(array: &[Value], start: i64, end: i64) -> Vec<Value> {
        let len = array.len() as i64;
        let start = start.max(0).min(len) as usize;
        let end = end.max(0).min(len) as usize;
        let end = end.max(start);
        array[start..end].to_vec()
    }

    fn join(vm: &BexVm, array: &[Value], separator: &str) -> String {
        array
            .iter()
            .map(|v| vm.as_string(v).cloned().unwrap_or_default())
            .collect::<Vec<_>>()
            .join(separator)
    }

    fn map(vm: &mut BexVm, array: &[Value], f: &Value) -> NativeCallResult {
        let Value::Object(f_ptr) = f else {
            return NativeCallResult::from(crate::errors::VmInternalError::TypeError {
                expected: bex_vm_types::types::Type::Object(bex_vm_types::ObjectType::Any),
                got: vm.type_of(f),
            });
        };
        let f_ptr = *f_ptr;
        let array = array.to_vec();

        if array.is_empty() {
            return NativeCallResult::Done(vm.alloc_array(vec![]));
        }

        let first_arg = array[0];
        let capacity = array.len();
        NativeCallResult::YieldToCall {
            callee: f_ptr,
            args: vec![first_arg],
            type_args: vec![],
            continuation: Box::new(MapContinuation {
                f_ptr,
                array,
                idx: 0,
                results: Vec::with_capacity(capacity),
            }),
        }
    }

    fn to_json(vm: &mut BexVm, array: &[Value]) -> NativeCallResult {
        if array.is_empty() {
            return NativeCallResult::Done(vm.alloc_array(vec![]));
        }

        let array = array.to_vec();
        let first_val = array[0];
        let callee = match make_to_json_callee(vm, first_val) {
            Ok(c) => c,
            Err(e) => return NativeCallResult::Error(e),
        };

        let capacity = array.len();
        NativeCallResult::YieldToCall {
            callee,
            args: vec![],
            type_args: vec![],
            continuation: Box::new(ToJsonContinuation {
                array,
                idx: 0,
                results: Vec::with_capacity(capacity),
            }),
        }
    }
}
