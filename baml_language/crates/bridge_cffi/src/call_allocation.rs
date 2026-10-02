//! Runtime-bound call allocations for BEP-81's revision-3 entry path.
//!
//! Allocation pins the runtime before encoding. Submission transfers retirement
//! to a lease; failures and completion retire that lease exactly once.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, PoisonError},
};

use bex_project::{Bex, CallId, CancellationToken, FunctionCallContextBuilder};

use crate::BridgeError;

struct Allocation {
    owner: Arc<dyn Bex>,
    cancel: CancellationToken,
    submitted: bool,
}

/// Owns call IDs from allocation through abandonment or actual completion.
/// Unknown/retired IDs never create cancellation state.
#[derive(Default)]
pub struct CallAllocationTable {
    calls: Mutex<HashMap<u64, Arc<Mutex<Allocation>>>>,
}

impl CallAllocationTable {
    pub fn allocate(&self, owner: Arc<dyn Bex>) -> Result<u64, BridgeError> {
        // Validate the owning runtime's clock before publishing the allocation.
        owner.invocation_clock_ns()?;
        let id = CallId::next().0;
        if id == 0 {
            return Err(BridgeError::InvocationProtocol(
                "call identifier space exhausted".into(),
            ));
        }
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                id,
                Arc::new(Mutex::new(Allocation {
                    owner,
                    cancel: CancellationToken::new(),
                    submitted: false,
                })),
            );
        Ok(id)
    }

    pub fn owner(&self, id: u64) -> Result<Arc<dyn Bex>, BridgeError> {
        let calls = self.calls.lock().unwrap_or_else(PoisonError::into_inner);
        let allocation = calls
            .get(&id)
            .ok_or(BridgeError::UnknownCallAllocation(id))?;
        let allocation = allocation.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(Arc::clone(&allocation.owner))
    }

    pub fn clock_ns(&self, id: u64) -> Result<u64, BridgeError> {
        let calls = self.calls.lock().unwrap_or_else(PoisonError::into_inner);
        let allocation = calls
            .get(&id)
            .ok_or(BridgeError::UnknownCallAllocation(id))?;
        let allocation = allocation.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(allocation.owner.invocation_clock_ns()?)
    }

    pub fn cancel(&self, id: u64) -> bool {
        let calls = self.calls.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(allocation) = calls.get(&id) else {
            return false;
        };
        allocation
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .cancel
            .cancel();
        true
    }

    /// Release only an unsubmitted allocation. A submitted call's lease owns
    /// retirement, including preparation failures after submission.
    pub fn release(&self, id: u64) -> bool {
        let mut calls = self.calls.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(allocation) = calls.get(&id) else {
            return false;
        };
        if allocation
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .submitted
        {
            return false;
        }
        calls.remove(&id);
        true
    }

    /// Consume an allocation once, using its original runtime even if the
    /// installed process-wide runtime changed in the meantime.
    pub fn submit(self: &Arc<Self>, id: u64) -> Result<SubmittedCall, BridgeError> {
        let calls = self.calls.lock().unwrap_or_else(PoisonError::into_inner);
        let allocation = calls
            .get(&id)
            .ok_or(BridgeError::UnknownCallAllocation(id))?;
        let mut state = allocation.lock().unwrap_or_else(PoisonError::into_inner);
        if state.submitted {
            return Err(BridgeError::DuplicateCallId(id));
        }
        state.submitted = true;
        Ok(SubmittedCall {
            id,
            owner: Arc::clone(&state.owner),
            cancel: state.cancel.clone(),
            allocation: Arc::clone(allocation),
            table: Arc::clone(self),
        })
    }
}

/// Pins the original runtime and retires the allocation on every terminal
/// path, including preparation failure and unwind. It does not own callbacks
/// that continue after the BAML result; those have separate execution leases.
pub struct SubmittedCall {
    id: u64,
    owner: Arc<dyn Bex>,
    cancel: CancellationToken,
    allocation: Arc<Mutex<Allocation>>,
    table: Arc<CallAllocationTable>,
}

impl SubmittedCall {
    pub fn owner(&self) -> &Arc<dyn Bex> {
        &self.owner
    }

    pub fn context_builder(&self) -> FunctionCallContextBuilder {
        FunctionCallContextBuilder::new(CallId(self.id)).with_cancel_token(self.cancel.clone())
    }
}

impl Drop for SubmittedCall {
    fn drop(&mut self) {
        let mut calls = self
            .table
            .calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if calls
            .get(&self.id)
            .is_some_and(|current| Arc::ptr_eq(current, &self.allocation))
        {
            calls.remove(&self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use sys_native::SysOpsExt as _;

    use super::*;

    fn runtime() -> Arc<dyn Bex> {
        bex_project::new(
            vfs::VfsPath::new(vfs::MemoryFS::new()),
            sys_ops::SysOps::native(),
            HashMap::from([(
                bex_project::FsPath::from_str("main.baml".into()),
                "function identity(value: int) -> int throws never { value }".into(),
            )]),
        )
        .unwrap()
    }

    #[test]
    fn allocation_pins_its_original_runtime_until_release() {
        let table = Arc::new(CallAllocationTable::default());
        let original = runtime();
        let weak = Arc::downgrade(&original);
        let id = table.allocate(original.clone()).unwrap();
        let replacement = runtime();
        assert!(!Arc::ptr_eq(&original, &replacement));
        drop(original);
        assert!(weak.upgrade().is_some());
        let submitted = table.submit(id).unwrap();
        assert!(Arc::ptr_eq(submitted.owner(), &weak.upgrade().unwrap()));
        assert!(!Arc::ptr_eq(submitted.owner(), &replacement));
        assert!(!table.release(id));
        drop(submitted);
        assert!(weak.upgrade().is_none());
        assert!(table.clock_ns(id).is_err());
    }

    #[test]
    fn pending_cancellation_survives_submission_without_native_pending_entries() {
        let table = Arc::new(CallAllocationTable::default());
        let id = table.allocate(runtime()).unwrap();
        assert!(table.cancel(id));
        assert!(table.cancel(id));
        let submitted = table.submit(id).unwrap();
        assert!(submitted.context_builder().build().cancel.is_cancelled());
        drop(submitted);
        assert!(!table.cancel(id));
        assert!(!table.release(id));
        assert!(table.submit(id).is_err());
    }

    #[test]
    fn allocation_is_consumed_once_and_preparation_failure_retires_it() {
        let table = Arc::new(CallAllocationTable::default());
        let id = table.allocate(runtime()).unwrap();
        let submitted = table.submit(id).unwrap();
        assert!(
            matches!(table.submit(id), Err(BridgeError::DuplicateCallId(found)) if found == id)
        );
        // A decoder/type validation error unwinds the same lease as completion.
        drop(submitted);
        assert!(table.submit(id).is_err());
        assert!(!table.cancel(id));
    }

    #[test]
    fn encoding_failure_releases_unsubmitted_allocation_exactly_once() {
        let table = Arc::new(CallAllocationTable::default());
        let id = table.allocate(runtime()).unwrap();
        assert!(table.release(id));
        assert!(!table.release(id));
        assert!(table.clock_ns(id).is_err());
        assert!(table.submit(id).is_err());
        assert!(!table.cancel(id));
        assert!(!table.cancel(0));
        assert!(!table.cancel(u64::MAX));
    }

    #[test]
    fn cancellation_racing_submission_or_completion_never_recreates_an_allocation() {
        let table = Arc::new(CallAllocationTable::default());
        let owner = runtime();
        for _ in 0..64 {
            let id = table.allocate(owner.clone()).unwrap();
            std::thread::scope(|scope| {
                scope.spawn(|| {
                    table.cancel(id);
                });
                let submitted = table.submit(id).unwrap();
                scope.spawn(|| {
                    table.cancel(id);
                });
                drop(submitted);
            });
            assert!(!table.cancel(id));
            assert!(table.clock_ns(id).is_err());
            assert!(table.submit(id).is_err());
        }
    }
}
