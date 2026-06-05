#![allow(unsafe_code)]

//! Native implementations of `baml.mock` (BEP-058).
//!
//! Slice 1 keys mocks by the mocked free function's fully-qualified name, which
//! is recoverable both here (`__new`, from the function value passed in) and at
//! the `Call` dispatch hook (from the callee function object). The mock's state
//! (replacement callable + atomic call counter) lives on the heap `Object::Mock`.

use bex_vm_types::{
    FunctionKey,
    types::{Object, Value},
};

use super::{BamlClassMockMock, BamlNamespaceMock, PackageBamlImpl};
use crate::{
    BexVm,
    errors::{VmPanic, VmRustFnError},
};

/// Reach into a value to get `&Mock`. `None` if it isn't an `Object::Mock`.
fn as_mock(value: &Value) -> Option<&bex_vm_types::Mock> {
    let ptr = value.as_object_ptr()?;
    // SAFETY: native functions run inline on the VM thread while the heap permit
    // is held; the object is not concurrently moved by GC.
    match unsafe { ptr.get() } {
        Object::Mock(m) => Some(m),
        _ => None,
    }
}

/// The fully-qualified name of the function `function_ptr` points at.
fn function_name(vm: &BexVm, function_ptr: bex_vm_types::HeapPtr) -> Option<String> {
    match vm.get_object(function_ptr) {
        Object::Function(f) => Some(f.name.clone()),
        _ => None,
    }
}

/// Build the `FunctionKey` for the callable `value` refers to. Free functions
/// and unbound class methods key on the function name; a bound method keys on
/// its receiver + method name. A value with no stable function identity (e.g. a
/// bare lambda) yields an empty `Free` key that never matches (rejected at
/// compile time in a later slice).
fn key_for(vm: &BexVm, value: Value) -> FunctionKey {
    let Some(ptr) = value.as_object_ptr() else {
        return FunctionKey::Free(String::new());
    };
    match vm.get_object(ptr) {
        Object::Function(f) => FunctionKey::Free(f.name.clone()),
        Object::Closure(c) => FunctionKey::Free(function_name(vm, c.function).unwrap_or_default()),
        Object::BoundMethod(bm) => {
            let name = function_name(vm, bm.function).unwrap_or_default();
            match bm.receiver.as_object_ptr() {
                Some(recv) => FunctionKey::Instance(recv, name),
                None => FunctionKey::Free(name),
            }
        }
        Object::GenericFunction(gf) => {
            let base = vm.globals.get(vm.proof(), gf.function);
            let name = base
                .as_object_ptr()
                .and_then(|p| function_name(vm, p))
                .unwrap_or_default();
            FunctionKey::Generic(name, gf.type_args.to_vec())
        }
        // An interface method value (`Animal.speak`) carries the interface
        // method's FQ identity; key the mock on it (BEP-058 slice 3).
        Object::InterfaceMethodRef(s) => FunctionKey::Free(s.to_string()),
        _ => FunctionKey::Free(String::new()),
    }
}

impl BamlClassMockMock for PackageBamlImpl {
    fn replace(vm: &mut BexVm, mock: &Value, replacement: &Value) {
        let Some(mock_ptr) = mock.as_object_ptr() else {
            return;
        };
        if let Some(m) = as_mock(mock) {
            m.set_replacement(*replacement);
            // The replacement is a freshly-allocated (young) callable being
            // stored into a Mock that may already have been promoted to an older
            // generation. Record the cross-generation edge so a minor GC traces
            // the replacement, exactly as StoreField / Cell stores do.
            vm.heap.write_barrier(mock_ptr, *replacement);
        }
    }
}

impl BamlNamespaceMock for PackageBamlImpl {
    fn __new(vm: &mut BexVm, original: &Value) -> Result<Value, VmRustFnError> {
        let key = key_for(vm, *original);
        // BEP-058 slice 10: the mock machinery itself is a non-mockable runtime
        // internal — intercepting it would corrupt the mock system or recurse.
        let name = match &key {
            FunctionKey::Free(n) | FunctionKey::Instance(_, n) | FunctionKey::Generic(n, _) => {
                n.as_str()
            }
        };
        if name.starts_with("baml.mock.") {
            return Err(VmRustFnError::Panic(VmPanic::UserPanic {
                message: format!(
                    "cannot mock `{name}`: the mock machinery is a non-mockable runtime internal"
                ),
            }));
        }
        Ok(Value::object(
            vm.tlab.alloc_mock(bex_vm_types::Mock::new(key)),
        ))
    }

    fn __enter(vm: &mut BexVm, mocks: &Value) {
        // BEP-058: `scope` accepts a single `Mock` or an array of `Mock`s; an
        // array activates each in order, so later elements sit innermost.
        for ptr in scope_mock_ptrs(vm, *mocks) {
            let key = match vm.get_object(ptr) {
                Object::Mock(m) => m.function_key.clone(),
                _ => continue,
            };
            // Reset the counter only on the first activation. Re-entering an
            // already-active mock (a nested scope of the same mock) must keep the
            // count accumulating across the nesting; it is reset again only once
            // the mock fully leaves scope and is re-entered fresh.
            let already_active = vm
                .mock_table
                .get(&key)
                .is_some_and(|stack| stack.contains(&ptr));
            if !already_active && let Object::Mock(m) = vm.get_object(ptr) {
                m.reset_call_count();
            }
            vm.mock_table.entry(key).or_default().push(ptr);
        }
    }

    fn __exit(vm: &mut BexVm, mocks: &Value) {
        // Deactivate in reverse activation order so the stack unwinds LIFO.
        for ptr in scope_mock_ptrs(vm, *mocks).into_iter().rev() {
            let key = match vm.get_object(ptr) {
                Object::Mock(m) => m.function_key.clone(),
                _ => continue,
            };
            if let Some(stack) = vm.mock_table.get_mut(&key) {
                stack.pop();
                if stack.is_empty() {
                    vm.mock_table.remove(&key);
                }
            }
        }
    }
}

/// The mock pointers a `scope` argument activates: a single `Object::Mock`, or
/// every `Object::Mock` element of an `Object::Array` (in array order).
fn scope_mock_ptrs(vm: &BexVm, value: Value) -> Vec<bex_vm_types::HeapPtr> {
    let Some(ptr) = value.as_object_ptr() else {
        return Vec::new();
    };
    match vm.get_object(ptr) {
        Object::Mock(_) => vec![ptr],
        Object::Array(arr) => arr
            .lock()
            .iter()
            .filter_map(|v| {
                let p = v.as_object_ptr()?;
                matches!(vm.get_object(p), Object::Mock(_)).then_some(p)
            })
            .collect(),
        _ => Vec::new(),
    }
}
