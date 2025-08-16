use std::future::Future;
use std::pin::Pin;

#[cfg(not(target_arch = "wasm32"))]
use tokio::runtime::Runtime;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use once_cell::sync::Lazy;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;

/// Platform-agnostic async runtime abstraction
pub struct AsyncRuntime;

#[cfg(not(target_arch = "wasm32"))]
static RUNTIME: Lazy<Arc<Runtime>> = Lazy::new(|| {
    Arc::new(Runtime::new().expect("Failed to create Tokio runtime"))
});

impl AsyncRuntime {
    /// Spawn a future on the appropriate runtime for the current platform
    pub fn spawn<F>(future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        #[cfg(not(target_arch = "wasm32"))]
        {
            RUNTIME.spawn(future);
        }
        
        #[cfg(target_arch = "wasm32")]
        {
            // spawn_local doesn't require Send, so we can directly use it
            spawn_local(future);
        }
    }
    
    /// Spawn a local future (for WASM compatibility)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn_local<F>(future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        RUNTIME.spawn(future);
    }
    
    /// Spawn a local future (for WASM compatibility)
    #[cfg(target_arch = "wasm32")]
    pub fn spawn_local<F>(future: F)
    where
        F: Future<Output = ()> + 'static,
    {
        spawn_local(future);
    }
    
    /// Get a handle to the native runtime (only available on non-WASM)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn native_runtime() -> Arc<Runtime> {
        RUNTIME.clone()
    }
}