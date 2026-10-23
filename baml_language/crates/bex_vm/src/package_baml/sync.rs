//! Native implementations for `namespace baml.sync`: `Lock` and `Guard`,
//! `Semaphore` and `Permit`. The lock itself is [`LockInner`] and a semaphore
//! is a [`LimitInner`], the same admission state `spawn with` uses; the waits
//! for a contended one are the `_wait` and `_wait_permit` sys-ops.

use std::{
    num::{NonZeroU64, NonZeroUsize},
    sync::Arc,
};

use bex_heap::TlabHolder;
use bex_vm_types::{
    GuardState, LimitInner, LockInner, Object, PermitState, RustDataArc as _, types::Value,
};

use super::{
    BamlClassSyncGuard, BamlClassSyncLock, BamlClassSyncPermit, BamlClassSyncSemaphore,
    BamlNamespaceSync, PackageBamlImpl,
    spawn::{clamp_to_i64, limit_inner},
    view,
};
use crate::{
    BexVm,
    errors::{VmBamlError, VmPanic, VmRustFnError},
};

/// The locks a VM's task has entered and not yet exited, one entry per
/// `_enter`, waits included.
///
/// `with` and `acquire` pair every `_enter` with an `_exit` (through `defer`)
/// or a guard, so entries left when the VM is dropped belong to a task that
/// ended without releasing: holds it kept (a guard never released, an aborted
/// unwind) and waits it never finished. Dropping releases them, so no lock
/// stays held by a task that no longer exists.
pub(crate) struct HeldLocks {
    task: NonZeroU64,
    entries: Vec<Arc<LockInner>>,
}

impl HeldLocks {
    pub(crate) fn new(task: NonZeroU64) -> Self {
        Self {
            task,
            entries: Vec::new(),
        }
    }

    fn enter(&mut self, lock: &Arc<LockInner>) -> Option<Arc<bex_vm_types::Waiter>> {
        let waiter = lock.enter(self.task);
        self.entries.push(Arc::clone(lock));
        waiter
    }

    fn exit(&mut self, lock: &Arc<LockInner>) {
        lock.exit(self.task);
        // The latest entry for this lock: `defer` unwinds them in reverse.
        if let Some(at) = self
            .entries
            .iter()
            .rposition(|entry| Arc::ptr_eq(entry, lock))
        {
            self.entries.swap_remove(at);
        }
    }
}

impl Drop for HeldLocks {
    fn drop(&mut self) {
        let mut entries = std::mem::take(&mut self.entries);
        entries.sort_unstable_by_key(Arc::as_ptr);
        entries.dedup_by(|a, b| Arc::ptr_eq(a, b));
        for lock in entries {
            lock.release_all(self.task);
        }
    }
}

/// The `Arc<T>` behind a native class's `_handle` field (field 0).
fn handle<T: bex_vm_types::BexRustData>(vm: &BexVm, instance: Value) -> Arc<T> {
    let field = vm
        .as_instance(&instance)
        .expect("a baml.sync value is a class instance")
        .load_field(0);
    match field.as_object_ptr().map(|ptr| vm.get_object(ptr)) {
        Some(Object::RustData(data)) => Arc::clone(data)
            .downcast_payload::<T>()
            .unwrap_or_else(|_| unreachable!("only the class's own natives set its handle")),
        _ => unreachable!("only the class's own natives set its handle"),
    }
}

fn alloc_handle(
    vm: &mut BexVm,
    item: baml_base::LangItem,
    data: Arc<impl bex_vm_types::BexRustData>,
) -> Value {
    let class = vm.lang_object(item);
    let handle = Value::object(vm.alloc_rust_data(data));
    Value::object(vm.alloc_instance(class, vec![handle]))
}

#[allow(clippy::used_underscore_items)]
impl BamlClassSyncLock for PackageBamlImpl {
    // Static constructor: returns the heap `Lock` instance, not `Self`.
    #[allow(clippy::new_ret_no_self)]
    fn new(vm: &mut BexVm) -> Value {
        alloc_handle(vm, crate::lang_items::BAML_SYNC_LOCK, LockInner::new())
    }

    fn _enter(vm: &mut BexVm, lock: &Value) -> Option<Value> {
        let lock = handle::<LockInner>(vm, *lock);
        let waiter = vm.held_locks.enter(&lock)?;
        Some(Value::object(vm.alloc_rust_data(waiter)))
    }

    fn _exit(vm: &mut BexVm, lock: &Value) {
        let lock = handle::<LockInner>(vm, *lock);
        vm.held_locks.exit(&lock);
    }

    fn _guard(vm: &mut BexVm, lock: &Value) -> Value {
        let lock = handle::<LockInner>(vm, *lock);
        let guard = GuardState::new(lock, vm.held_locks.task);
        alloc_handle(vm, crate::lang_items::BAML_SYNC_GUARD, Arc::new(guard))
    }
}

impl BamlClassSyncGuard for PackageBamlImpl {
    fn release(vm: &mut BexVm, guard: &Value) -> Result<(), VmRustFnError> {
        let guard = handle::<GuardState>(vm, *guard);
        match guard.claim(vm.held_locks.task) {
            Ok(true) => {
                vm.held_locks.exit(guard.lock());
                Ok(())
            }
            Ok(false) => Ok(()),
            Err(bex_vm_types::NotHeld) => Err(VmPanic::LockNotHeld {
                message: "baml.sync.Guard.release: called from a task other than the one \
                          that acquired it, while that task still holds the lock"
                    .to_string(),
            }
            .into()),
        }
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

#[allow(clippy::used_underscore_items)]
impl BamlNamespaceSync for PackageBamlImpl {
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
