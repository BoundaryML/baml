//! Finalize a dynamic engine after the last registration, call, and capability owner releases it.
use bex_project::{
    Bex, BexArgs, BexCallTraceResult, BexExternalValue, CallId, FunctionCallContext, Handle,
    RuntimeError, UnhandledSpawnErrorHandler,
};
use std::sync::{Arc, LazyLock, Mutex, Weak};

static RETIRING: LazyLock<Mutex<Vec<Weak<dyn Bex>>>> = LazyLock::new(|| Mutex::new(Vec::new()));

pub(crate) fn retiring_runtimes() -> Vec<Arc<dyn Bex>> {
    RETIRING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .filter_map(Weak::upgrade)
        .collect()
}

pub(crate) fn own_dynamic(inner: Arc<dyn Bex>) -> Arc<dyn Bex> {
    Arc::new(DynamicRuntime { inner })
}
struct DynamicRuntime {
    inner: Arc<dyn Bex>,
}

#[async_trait::async_trait]
impl Bex for DynamicRuntime {
    async fn call_function(
        self: Arc<Self>,
        name: &str,
        args: BexArgs,
        ctx: FunctionCallContext,
    ) -> Result<BexExternalValue, RuntimeError> {
        self.inner.clone().call_function(name, args, ctx).await
    }
    async fn call_callable(
        self: Arc<Self>,
        handle: Handle,
        args: BexArgs,
        ctx: FunctionCallContext,
    ) -> Result<BexExternalValue, RuntimeError> {
        self.inner.clone().call_callable(handle, args, ctx).await
    }
    async fn call_function_with_trace(
        self: Arc<Self>,
        name: &str,
        args: BexArgs,
        ctx: FunctionCallContext,
    ) -> Result<BexCallTraceResult, RuntimeError> {
        self.inner
            .clone()
            .call_function_with_trace(name, args, ctx)
            .await
    }
    fn cancel_function_call(&self, id: CallId) -> Result<(), RuntimeError> {
        self.inner.cancel_function_call(id)
    }
    fn set_unhandled_spawn_error_handler(&self, handler: Option<UnhandledSpawnErrorHandler>) {
        self.inner.set_unhandled_spawn_error_handler(handler);
    }
    async fn shutdown(self: Arc<Self>) {
        self.inner.clone().shutdown().await;
    }
}

impl Drop for DynamicRuntime {
    fn drop(&mut self) {
        let inner = self.inner.clone();
        {
            let mut retiring = RETIRING
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            retiring.retain(|runtime| runtime.strong_count() != 0);
            retiring.push(Arc::downgrade(&inner));
        }
        let finish = async move {
            inner.clone().shutdown().await;
            drop(inner);
            // Dropping the heap may enqueue host-callable releases after the final GC.
            bex_project::host_release_dispatch::drain();
        };
        #[cfg(not(target_arch = "wasm32"))]
        if let Ok(runtime) = crate::get_tokio_runtime() {
            runtime.spawn(finish);
        }
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(finish);
    }
}
