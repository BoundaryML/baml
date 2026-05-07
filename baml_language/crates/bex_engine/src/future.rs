use ::bex_heap::{
    HeapPermit, PermitProof, SharedHeapPermit, SharedHeapPermitGuard, Tlab, TlabHolder,
};
use ::bex_vm_types::{
    HeapPtr, Object, ObjectType, RootHaver, Value,
    types::{FutureId, FutureType},
};
use ::core::sync::atomic::AtomicUsize;
use ::std::{collections::HashMap, sync::Arc};
use ::sys_types::CancellationToken;

use crate::EngineError;

/// Manages all futures for the Bex engine.
///
/// This is a shared resource managed using a [`SharedHeapPermit`].
pub struct FutureManager {
    inner: SharedHeapPermit<FutureManagerInner>,
}

impl FutureManager {
    pub fn new(inner: SharedHeapPermit<FutureManagerInner>) -> Self {
        Self { inner }
    }
    pub async fn acquire(&self) -> FutureManagerGuard<'_> {
        FutureManagerGuard {
            inner: self.inner.acquire().await,
        }
    }
}

pub struct FutureManagerGuard<'a> {
    inner: SharedHeapPermitGuard<'a, FutureManagerInner>,
}

impl FutureManagerGuard<'_> {
    /// Registers a future with the future manager and returns a unique ID.
    pub fn new_future(&mut self, cancel: CancellationToken) -> (FutureId, HeapPtr) {
        let id = self
            .inner
            .next_future_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // SAFETY: we are the only one allowed to create new future ids
        let id = unsafe { FutureId::from_usize(id) };

        let ptr = self
            .inner
            .tlab
            .alloc_future(::bex_vm_types::Future::Pending(id));

        let future_state = FutureState {
            future: ptr,
            ready: Arc::new(tokio::sync::SetOnce::new()),
            cancel,
        };
        self.inner.active_futures.insert(id, future_state);
        (id, ptr)
    }
    pub fn fulfill_future(&mut self, id: FutureId, value: Value) -> Result<(), EngineError> {
        let future = self.inner.get_future_mut(id)?;
        // SAFETY: We shold a heap permit in `self`.
        let fut = unsafe { future.get_mut() }?;
        if !matches!(fut, bex_vm_types::Future::Pending(_)) {
            return Err(EngineError::TypeMismatch {
                message: format!("Expected Pending Future, got {:?}", FutureType::of(fut)),
            });
        }

        *fut = bex_vm_types::Future::Ready(value);
        let set = future.ready.set(Ok(()));
        debug_assert!(
            set.is_ok(),
            "Should not have been ready if the heap future was pending."
        );
        Ok(())
    }
    pub fn err_future(&mut self, id: FutureId, err: Value) -> Result<(), EngineError> {
        let future = self.inner.get_future_mut(id)?;
        // SAFETY: We shold a heap permit in `self`.
        let fut = unsafe { future.get_mut() }?;
        if !matches!(fut, bex_vm_types::Future::Pending(_)) {
            return Err(EngineError::TypeMismatch {
                message: format!("Expected Pending Future, got {:?}", FutureType::of(fut)),
            });
        }

        *fut = bex_vm_types::Future::Error(err);
        let set = future.ready.set(Ok(()));
        debug_assert!(
            set.is_ok(),
            "Should not have been ready if the heap future was pending."
        );
        Ok(())
    }
    pub fn cancel_future(&mut self, id: FutureId) -> Result<(), EngineError> {
        let future = self.inner.get_future_mut(id)?;
        // SAFETY: We shold a heap permit in `self`.
        let fut = unsafe { future.get_mut() }?;
        if !matches!(fut, bex_vm_types::Future::Pending(_)) {
            return Err(EngineError::TypeMismatch {
                message: format!("Expected Pending Future, got {:?}", FutureType::of(fut)),
            });
        }

        *fut = bex_vm_types::Future::Cancelled;
        let set = future.ready.set(Ok(()));
        debug_assert!(
            set.is_ok(),
            "Should not have been ready if the heap future was pending."
        );
        future.cancel.cancel();
        Ok(())
    }
    /// Sets the future to `InternalError` and notifies the waiter.
    pub fn internal_error_future(
        &mut self,
        id: FutureId,
        err: EngineError,
    ) -> Result<(), EngineError> {
        let future = self.inner.get_future_mut(id)?;
        // SAFETY: We shold a heap permit in `self`.
        let fut = unsafe { future.get_mut() }?;
        if !matches!(fut, bex_vm_types::Future::Pending(_)) {
            return Err(EngineError::TypeMismatch {
                message: format!("Expected Pending Future, got {:?}", FutureType::of(fut)),
            });
        }

        *fut = bex_vm_types::Future::InternalError;
        let set = future.ready.set(Err(err));
        debug_assert!(
            set.is_ok(),
            "Should not have been ready if the heap future was pending."
        );
        Ok(())
    }
    /// Returns a Rust future that will resolve when the BAML future is ready.
    /// Once it is resolved, the future on the heap will be ready (in some form).
    ///
    /// ## Errors
    /// - Synchronous `EngineError::FutureNotFound` if the future is not found
    /// - Future returns `EngineError` if the future produces an `InternalError`
    pub fn future_ready(
        &self,
        id: FutureId,
    ) -> Result<impl Future<Output = Result<(), EngineError>> + use<>, EngineError> {
        let future = self
            .inner
            .active_futures
            .get(&id)
            .ok_or(EngineError::FutureNotFound { future_id: id })?;
        let waiter = Arc::clone(&future.ready);
        Ok(async move { waiter.wait().await.clone() })
    }
}
impl TlabHolder for FutureManagerGuard<'_> {
    fn tlab(&self) -> &Tlab {
        self.inner.tlab()
    }
    fn tlab_mut(&mut self) -> &mut Tlab {
        self.inner.tlab_mut()
    }
}
impl HeapPermit<FutureManagerInner> for FutureManagerGuard<'_> {
    fn holder(&self) -> &FutureManagerInner {
        &self.inner
    }
    fn holder_mut(&mut self) -> &mut FutureManagerInner {
        &mut self.inner
    }
    fn proof(&self) -> PermitProof<'_> {
        self.inner.proof()
    }
}

pub struct FutureManagerInner {
    tlab: Tlab,
    next_future_id: AtomicUsize,
    active_futures: HashMap<FutureId, FutureState>,
}
impl FutureManagerInner {
    pub fn new(tlab: Tlab) -> Self {
        Self {
            tlab,
            next_future_id: AtomicUsize::new(0),
            active_futures: HashMap::new(),
        }
    }
    #[expect(dead_code)]
    fn get_future(&self, id: FutureId) -> Result<&FutureState, EngineError> {
        self.active_futures
            .get(&id)
            .ok_or(EngineError::FutureNotFound { future_id: id })
    }
    fn get_future_mut(&mut self, id: FutureId) -> Result<&mut FutureState, EngineError> {
        self.active_futures
            .get_mut(&id)
            .ok_or(EngineError::FutureNotFound { future_id: id })
    }
}
impl RootHaver for FutureManagerInner {
    fn collect_roots(&self, roots: &mut Vec<HeapPtr>) {
        // blocking is fine since we should only ever call this while holding exclusive heap access
        for future in self.active_futures.values() {
            future.collect_roots(roots);
        }
    }
    fn forward_roots(&mut self, roots: &HashMap<HeapPtr, HeapPtr>) {
        for future in self.active_futures.values_mut() {
            future.forward_roots(roots);
        }
    }
}
impl TlabHolder for FutureManagerInner {
    fn tlab(&self) -> &Tlab {
        &self.tlab
    }
    fn tlab_mut(&mut self) -> &mut Tlab {
        &mut self.tlab
    }
}

struct FutureState {
    future: HeapPtr,
    /// Set once the `Future` object is no longer `Pending`
    /// - `Ok(())` means there is a BAML value ready on the heap
    /// - `Err(err)` means it's `InternalError` and `err` is the error value
    ready: Arc<tokio::sync::SetOnce<Result<(), EngineError>>>,
    pub cancel: CancellationToken,
}
impl FutureState {
    /// SAFETY: We must hold a heap permit for the duration of the future object.
    #[expect(dead_code)]
    unsafe fn get(&self) -> Result<&bex_vm_types::Future, EngineError> {
        // SAFETY: We hold a permit, so we can access the future object.
        let obj = unsafe { self.future.get() };
        match obj {
            Object::Future(fut) => Ok(fut),
            other => Err(EngineError::TypeMismatch {
                message: format!("Expected Future, got {:?}", ObjectType::of(other)),
            }),
        }
    }
    /// SAFETY: We must hold a heap permit for the duration of the future object.
    unsafe fn get_mut(&mut self) -> Result<&mut bex_vm_types::Future, EngineError> {
        // SAFETY: We hold a permit, so we can access the future object.
        let obj = unsafe { self.future.get_mut() };
        match obj {
            Object::Future(fut) => Ok(fut),
            other => Err(EngineError::TypeMismatch {
                message: format!("Expected Future, got {:?}", ObjectType::of(other)),
            }),
        }
    }
}
impl RootHaver for FutureState {
    fn collect_roots(&self, roots: &mut Vec<HeapPtr>) {
        roots.push(self.future);
    }
    fn forward_roots(&mut self, roots: &HashMap<HeapPtr, HeapPtr>) {
        if let Some(new_result) = roots.get(&self.future) {
            self.future = *new_result;
        }
    }
}
