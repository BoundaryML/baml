//! Native implementations for `namespace baml.sync`: the key taking and
//! releasing behind `baml.sync.atomic`, and `Semaphore` and `Permit`. Held keys
//! live in the engine-wide [`MonitorTable`]; a semaphore is a [`LimitInner`],
//! the same admission state `spawn with` uses. The waits for contended keys or
//! a slot are the `_wait` and `_wait_permit` sys-ops.

use std::{
    num::{NonZeroU64, NonZeroUsize},
    sync::Arc,
};

use bex_heap::TlabHolder;
use bex_vm_types::{
    HeapPtr, LimitInner, MonitorTable, Object, PermitState, RootVisitor, RustDataArc as _,
    types::{Array, Value},
};

use super::{
    BamlClassSyncPermit, BamlClassSyncSemaphore, BamlNamespaceSync, PackageBamlImpl,
    spawn::{clamp_to_i64, limit_inner},
    view,
};
use crate::{
    BexVm,
    errors::{VmBamlError, VmInternalError, VmRustFnError},
};

/// A VM's view of the engine's locks: the table, the task that owns what this
/// VM locks, and how many of its `_enter` calls have not been matched by an
/// `_exit`.
///
/// `atomic` always pairs them through a `defer`, so a VM that ends with the
/// count above zero skipped a `defer` (an aborted unwind), and dropping it
/// releases whatever its task still holds or waits for.
pub(crate) struct TaskMonitors {
    table: Arc<MonitorTable>,
    task: NonZeroU64,
    unmatched: u32,
}

impl TaskMonitors {
    pub(crate) fn new(table: Arc<MonitorTable>, task: NonZeroU64) -> Self {
        Self {
            table,
            task,
            unmatched: 0,
        }
    }
}

/// The engine's GC participant for the lock table: forwards its keys after a
/// collection moves their objects, and drops those whose objects died. Holds
/// no roots: a held lock's object is already on its holder's stack.
pub struct MonitorRoot {
    table: Arc<MonitorTable>,
}

impl MonitorRoot {
    pub fn new(table: Arc<MonitorTable>) -> Self {
        Self { table }
    }
}

impl bex_vm_types::RootHaver for MonitorRoot {
    fn visit_roots(&mut self, visitor: &mut dyn RootVisitor) {
        self.table.visit_keys(visitor);
    }
}

impl Drop for TaskMonitors {
    fn drop(&mut self) {
        if self.unmatched > 0 {
            self.table.release_all(self.task);
        }
    }
}

/// The objects an `atomic` key names.
enum AtomicKey {
    One(HeapPtr),
    /// Every instance in a list, sorted, without duplicates.
    Many(Vec<HeapPtr>),
}

/// The objects `key` names: a class instance, or each instance in a list.
/// The key's type admits nothing else.
fn atomic_key(vm: &BexVm, key: Value) -> Result<AtomicKey, VmRustFnError> {
    let instance = |value: Value| {
        value
            .as_object_ptr()
            .filter(|&ptr| matches!(vm.get_object(ptr), Object::Instance(_)))
    };
    if let Some(ptr) = instance(key) {
        return Ok(AtomicKey::One(ptr));
    }
    if let Some(ptr) = key.as_object_ptr()
        && let Object::Array(list) = vm.get_object(ptr)
    {
        let mut keys = Vec::new();
        for item in list.to_vec() {
            keys.push(instance(item).ok_or_else(|| not_a_key(vm, item))?);
        }
        keys.sort_unstable();
        keys.dedup();
        return Ok(match keys.as_slice() {
            [one] => AtomicKey::One(*one),
            _ => AtomicKey::Many(keys),
        });
    }
    Err(not_a_key(vm, key))
}

fn not_a_key(vm: &BexVm, value: Value) -> VmRustFnError {
    VmInternalError::MalformedArgument {
        message: format!(
            "baml.sync.atomic: a key is a class instance or a list of them, got {}",
            vm.type_of(&value)
        ),
    }
    .into()
}

#[allow(clippy::used_underscore_items)]
impl BamlNamespaceSync for PackageBamlImpl {
    fn _held(vm: &mut BexVm, key: &Value) -> Result<Value, VmRustFnError> {
        let Some(ptr) = key.as_object_ptr() else {
            return Err(not_a_key(vm, *key));
        };
        let Object::Array(list) = vm.get_object(ptr) else {
            return Ok(*key);
        };
        let element_ty = list.element_ty.as_ref().clone();
        let items = list.to_vec();
        Ok(Value::object(
            vm.tlab.alloc(Object::Array(Array::new(element_ty, items))),
        ))
    }

    fn _enter(vm: &mut BexVm, key: &Value) -> Result<Option<Value>, VmRustFnError> {
        let key = atomic_key(vm, *key)?;
        let monitors = &mut vm.monitors;
        let waiter = match &key {
            AtomicKey::One(key) => monitors.table.enter(*key, monitors.task),
            AtomicKey::Many(keys) if keys.is_empty() => None,
            AtomicKey::Many(keys) => monitors.table.enter_all(keys, monitors.task),
        };
        monitors.unmatched += 1;
        Ok(waiter.map(|waiter| Value::object(vm.alloc_rust_data(waiter))))
    }

    fn _exit(vm: &mut BexVm, key: &Value) -> Result<(), VmRustFnError> {
        let key = atomic_key(vm, *key)?;
        let monitors = &mut vm.monitors;
        match &key {
            AtomicKey::One(key) => monitors.table.exit(*key, monitors.task),
            AtomicKey::Many(keys) if keys.is_empty() => {}
            AtomicKey::Many(keys) => monitors.table.exit_all(keys, monitors.task),
        }
        monitors.unmatched = monitors.unmatched.saturating_sub(1);
        Ok(())
    }

    fn _admitted(vm: &BexVm, held: &Value) -> bool {
        permit_state(vm, *held).is_admitted()
    }

    fn _permit(vm: &mut BexVm, held: &Value) -> Value {
        let held = permit_state(vm, *held);
        alloc_handle(vm, crate::lang_items::BAML_SYNC_PERMIT, held)
    }

    fn _release_permit(vm: &BexVm, held: &Value) {
        permit_state(vm, *held).release();
    }
}

/// The hold behind a `$rust_type` from `Semaphore._admit`.
fn permit_state(vm: &BexVm, held: Value) -> Arc<PermitState> {
    match held.as_object_ptr().map(|ptr| vm.get_object(ptr)) {
        Some(Object::RustData(data)) => Arc::clone(data)
            .downcast_payload::<PermitState>()
            .unwrap_or_else(|_| unreachable!("only `Semaphore._admit` makes a hold")),
        _ => unreachable!("only `Semaphore._admit` makes a hold"),
    }
}

#[allow(clippy::used_underscore_items)]
impl BamlClassSyncSemaphore for PackageBamlImpl {
    // Static constructor: returns the heap `Semaphore` instance, not `Self`.
    #[allow(clippy::new_ret_no_self)]
    fn new(vm: &mut BexVm, capacity: i64) -> Result<Value, VmRustFnError> {
        let capacity = usize::try_from(capacity)
            .ok()
            .and_then(NonZeroUsize::new)
            .ok_or_else(|| VmBamlError::InvalidArgument {
                message: format!(
                    "baml.sync.Semaphore.new: capacity must be positive, got {capacity}"
                ),
            })?;
        Ok(alloc_handle(
            vm,
            crate::lang_items::BAML_SYNC_SEMAPHORE,
            LimitInner::new(capacity),
        ))
    }

    fn capacity(vm: &BexVm, semaphore: &view::sync::Semaphore<'_>) -> i64 {
        clamp_to_i64(semaphore._handle::<LimitInner>(vm).capacity().get())
    }

    fn active_count(vm: &BexVm, semaphore: &view::sync::Semaphore<'_>) -> i64 {
        clamp_to_i64(semaphore._handle::<LimitInner>(vm).active_count())
    }

    fn queued_count(vm: &BexVm, semaphore: &view::sync::Semaphore<'_>) -> i64 {
        clamp_to_i64(semaphore._handle::<LimitInner>(vm).queued_count())
    }

    fn _admit(vm: &mut BexVm, semaphore: &Value) -> Value {
        let limit = limit_inner(vm, *semaphore)
            .unwrap_or_else(|_| unreachable!("only `Semaphore.new` sets its handle"));
        let held = Arc::new(PermitState::admit(&limit));
        Value::object(vm.alloc_rust_data(held))
    }
}

#[allow(clippy::used_underscore_items)]
impl BamlClassSyncPermit for PackageBamlImpl {
    fn release(vm: &BexVm, permit: &view::sync::Permit<'_>) {
        permit._handle::<PermitState>(vm).release();
    }
}

/// A native class instance (`class_item`) whose `_handle` field holds `data`.
fn alloc_handle(
    vm: &mut BexVm,
    class_item: baml_base::LangItem,
    data: Arc<impl bex_vm_types::BexRustData>,
) -> Value {
    let class = vm.lang_object(class_item);
    let handle = Value::object(vm.alloc_rust_data(data));
    Value::object(vm.alloc_instance(class, vec![handle]))
}
