use prost::Message;

use crate::{
    client_registry::ClientRegistry,
    codec::BamlEncode,
    error::BamlError,
    proto::baml_cffi_v1::{
        host_map_entry, BamlObjectHandle, HostEnvVar, HostFunctionArguments, HostMapEntry,
    },
    raw_objects::{Collector, RawObjectTrait, TypeBuilder},
};

/// Cancellation system using channels for efficient sync/async support.
///
/// Design:
/// - `CancellationToken`: Read-only handle for checking/waiting on cancellation
///   (user-facing)
/// - `CancellationSource`: Internal handle for triggering cancellation with
///   callbacks (runtime-only)
/// - `CancellationGuard`: Active cancellation context with callback support
///
/// Uses:
/// - `AtomicBool` for lock-free `is_cancelled()` checks
/// - `Condvar` for efficient sync blocking waits
/// - `async-channel` for async waiting
/// - Oneshot channel for callback notification
#[allow(dead_code)] // Internal types will be used when runtime is updated
pub(crate) mod cancellation {
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, Condvar, Mutex,
        },
        time::Duration,
    };

    /// Shared state between token and source
    struct SharedState {
        /// Fast atomic flag for lock-free checking
        cancelled: AtomicBool,
        /// Condvar for sync blocking waits
        condvar: Condvar,
        /// Mutex for condvar (value is unused, just needed for condvar API)
        mutex: Mutex<()>,
        /// Async channel sender for notifying async waiters
        async_sender: async_channel::Sender<()>,
        /// Async channel receiver for async waiting
        async_receiver: async_channel::Receiver<()>,
    }

    impl std::fmt::Debug for SharedState {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("SharedState")
                .field("cancelled", &self.cancelled)
                .finish()
        }
    }

    impl SharedState {
        fn new() -> Self {
            let (async_sender, async_receiver) = async_channel::bounded(1);
            Self {
                cancelled: AtomicBool::new(false),
                condvar: Condvar::new(),
                mutex: Mutex::new(()),
                async_sender,
                async_receiver,
            }
        }

        /// Check if cancelled (lock-free, very fast)
        #[inline]
        fn is_cancelled(&self) -> bool {
            self.cancelled.load(Ordering::Acquire)
        }

        /// Trigger cancellation, returns true if this was the first
        /// cancellation
        fn cancel(&self) -> bool {
            // swap returns the previous value; if it was false, we're the first to cancel
            if !self.cancelled.swap(true, Ordering::AcqRel) {
                // Notify all sync waiters
                self.condvar.notify_all();
                // Notify async waiters (ignore error if channel full/closed)
                let _ = self.async_sender.try_send(());
                true
            } else {
                false
            }
        }

        /// Block until cancelled (sync)
        fn wait(&self) {
            if self.is_cancelled() {
                return;
            }
            let guard = self.mutex.lock().unwrap();
            // Wait while NOT cancelled, then drop the guard
            drop(
                self.condvar
                    .wait_while(guard, |()| !self.is_cancelled())
                    .unwrap(),
            );
        }

        /// Block until cancelled with timeout (sync)
        /// Returns true if cancelled, false if timed out
        fn wait_timeout(&self, timeout: Duration) -> bool {
            if self.is_cancelled() {
                return true;
            }
            let guard = self.mutex.lock().unwrap();
            let (guard, result) = self
                .condvar
                .wait_timeout_while(guard, timeout, |()| !self.is_cancelled())
                .unwrap();
            drop(guard);
            // If we didn't time out, we were cancelled
            !result.timed_out()
        }
    }

    // =========================================================================
    // CancellationToken - User-facing handle
    // =========================================================================

    /// Token for checking, waiting on, and triggering cancellation.
    ///
    /// # Example
    /// ```ignore
    /// // Create a token
    /// let token = CancellationToken::new();
    ///
    /// // Clone to pass to other threads/tasks
    /// let token2 = token.clone();
    ///
    /// // Check if cancelled (fast, lock-free)
    /// if token.is_cancelled() {
    ///     return Err("operation cancelled");
    /// }
    ///
    /// // Trigger cancellation
    /// token.cancel();
    ///
    /// // Block until cancelled (sync)
    /// token.wait();
    ///
    /// // Wait with timeout
    /// if !token.wait_timeout(Duration::from_secs(5)) {
    ///     println!("timed out waiting for cancellation");
    /// }
    ///
    /// // Await cancellation (async)
    /// token.cancelled().await;
    /// ```
    #[derive(Debug, Clone)]
    pub struct CancellationToken {
        state: Arc<SharedState>,
        /// Timeout duration - only starts when runtime calls `on_cancel()`
        timeout: Option<Duration>,
    }

    impl CancellationToken {
        /// Create a new cancellation token.
        pub fn new() -> Self {
            Self {
                state: Arc::new(SharedState::new()),
                timeout: None,
            }
        }

        /// Create a new cancellation token with automatic timeout.
        ///
        /// **Important:** The timeout does NOT start immediately. It only
        /// starts when the token is passed to a runtime function
        /// (`call_function`, etc.). This allows you to create the token
        /// ahead of time without the clock ticking.
        pub fn new_with_timeout(timeout: Duration) -> Self {
            Self {
                state: Arc::new(SharedState::new()),
                timeout: Some(timeout),
            }
        }

        /// Check if cancelled.
        ///
        /// This is a lock-free atomic read, extremely fast for polling.
        #[inline]
        pub fn is_cancelled(&self) -> bool {
            self.state.is_cancelled()
        }

        /// Trigger cancellation.
        ///
        /// Safe to call multiple times - only the first call has effect.
        /// All waiters (sync and async) will be notified.
        pub fn cancel(&self) {
            self.state.cancel();
        }

        /// Block the current thread until cancellation occurs.
        ///
        /// If already cancelled, returns immediately.
        pub fn wait(&self) {
            self.state.wait();
        }

        /// Block until cancelled or timeout expires.
        ///
        /// Returns `true` if cancelled, `false` if the timeout elapsed first.
        pub fn wait_timeout(&self, timeout: Duration) -> bool {
            self.state.wait_timeout(timeout)
        }

        /// Await cancellation asynchronously.
        ///
        /// If already cancelled, returns immediately.
        pub async fn cancelled(&self) {
            if self.is_cancelled() {
                return;
            }
            // Wait for signal on async channel
            let _ = self.state.async_receiver.recv().await;
        }

        /// Register a callback to run when cancellation occurs.
        ///
        /// The callback runs on a dedicated thread and is invoked exactly once
        /// when the token is cancelled. If already cancelled, callback runs
        /// immediately.
        ///
        /// **Important:** If this token was created with `new_with_timeout()`,
        /// the timeout countdown starts NOW (when this method is
        /// called), not when the token was created.
        ///
        /// Returns a guard that, when dropped, signals the watcher thread to
        /// stop (if cancellation hasn't occurred yet).
        ///
        /// # Internal Use
        /// This is primarily for runtime integration - to hook FFI cleanup on
        /// cancellation.
        pub(crate) fn on_cancel<F>(&self, callback: F) -> OnCancelGuard
        where
            F: FnOnce() + Send + 'static,
        {
            // If already cancelled, run callback immediately
            if self.is_cancelled() {
                callback();
                return OnCancelGuard {
                    stop_tx: None,
                    watcher_thread: None,
                    timeout_thread: None,
                };
            }

            // Start timeout thread if configured - THIS is when the clock starts
            let timeout_thread = self.timeout.map(|timeout| {
                let state = self.state.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(timeout);
                    state.cancel();
                })
            });

            // Channel to signal the watcher thread to stop (when function completes)
            let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();

            let state = self.state.clone();
            let watcher_thread = std::thread::spawn(move || {
                loop {
                    // Check if cancelled
                    if state.is_cancelled() {
                        callback();
                        return;
                    }
                    // Wait for stop signal or timeout
                    match stop_rx.recv_timeout(Duration::from_millis(10)) {
                        Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                            // Stop signal received or sender dropped - don't run callback
                            return;
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                            // Continue polling
                        }
                    }
                }
            });

            OnCancelGuard {
                stop_tx: Some(stop_tx),
                watcher_thread: Some(watcher_thread),
                timeout_thread,
            }
        }
    }

    impl Default for CancellationToken {
        fn default() -> Self {
            Self::new()
        }
    }

    // =========================================================================
    // OnCancelGuard - Manages cancellation callback lifecycle
    // =========================================================================

    /// Guard that manages a cancellation callback.
    ///
    /// When dropped, signals the watcher thread to stop if the callback
    /// hasn't been invoked yet. This prevents the callback from firing
    /// after the operation has completed normally.
    pub(crate) struct OnCancelGuard {
        stop_tx: Option<std::sync::mpsc::Sender<()>>,
        watcher_thread: Option<std::thread::JoinHandle<()>>,
        timeout_thread: Option<std::thread::JoinHandle<()>>,
    }

    impl Drop for OnCancelGuard {
        fn drop(&mut self) {
            // Signal watcher to stop (if still running)
            if let Some(tx) = self.stop_tx.take() {
                let _ = tx.send(());
            }
            // Don't join threads - let them exit naturally
            // This avoids blocking on drop
            // Note: timeout thread will continue running but cancel() is
            // idempotent
        }
    }

    // =========================================================================
    // CancellationSource - Internal handle for runtime
    // =========================================================================

    /// Source for creating and managing cancellation.
    ///
    /// This is only accessible internally through the runtime.
    /// Users cannot create this directly - they only get `CancellationToken`.
    ///
    /// # Usage
    /// ```ignore
    /// // Create a source (only runtime can do this)
    /// let source = CancellationSource::new();
    ///
    /// // Give tokens to user code
    /// let token = source.token();
    ///
    /// // Start with a callback that runs on cancellation
    /// let guard = source.start(|| {
    ///     println!("Cancelled! Cleaning up...");
    /// });
    ///
    /// // Later, trigger cancellation
    /// guard.cancel();
    /// ```
    pub(crate) struct CancellationSource {
        state: Arc<SharedState>,
        timeout: Option<Duration>,
    }

    impl CancellationSource {
        /// Create a new cancellation source.
        pub(crate) fn new() -> Self {
            Self {
                state: Arc::new(SharedState::new()),
                timeout: None,
            }
        }

        /// Create a new cancellation source with automatic timeout.
        ///
        /// The cancellation will automatically trigger after the timeout
        /// expires once `start()` is called.
        pub(crate) fn with_timeout(timeout: Duration) -> Self {
            Self {
                state: Arc::new(SharedState::new()),
                timeout: Some(timeout),
            }
        }

        /// Get a token to pass to user code.
        ///
        /// Tokens are cheap to clone and can be passed to multiple tasks.
        /// Note: tokens from `CancellationSource` don't have their own timeout
        /// - the source manages timeout via `start()`.
        pub(crate) fn token(&self) -> CancellationToken {
            CancellationToken {
                state: self.state.clone(),
                timeout: None,
            }
        }

        /// Check if already cancelled.
        #[inline]
        pub(crate) fn is_cancelled(&self) -> bool {
            self.state.is_cancelled()
        }

        /// Trigger cancellation immediately.
        ///
        /// Note: If you need callback support, use `start()` first.
        pub(crate) fn cancel(&self) {
            self.state.cancel();
        }

        /// Start the cancellation source with a callback.
        ///
        /// The callback will be invoked exactly once when cancellation occurs,
        /// whether triggered manually via `guard.cancel()` or by timeout.
        ///
        /// # Arguments
        /// * `on_cancel` - Callback to invoke when cancellation happens
        ///
        /// # Returns
        /// A `CancellationGuard` that can be used to trigger cancellation
        /// and obtain additional tokens.
        pub(crate) fn start<F>(self, on_cancel: F) -> CancellationGuard
        where
            F: FnOnce() + Send + 'static,
        {
            let state = self.state.clone();

            // Channel to signal the callback thread to check for cancellation
            let (cancel_tx, cancel_rx) = std::sync::mpsc::channel::<()>();

            // Spawn callback thread that waits for cancellation
            let callback_state = self.state.clone();
            let callback_thread = std::thread::spawn(move || {
                // Wait for signal (either from cancel_tx or just poll the state)
                // We use the channel as a wakeup mechanism
                loop {
                    // Check if cancelled
                    if callback_state.is_cancelled() {
                        on_cancel();
                        return;
                    }
                    // Wait for wakeup signal with short timeout
                    // This allows us to detect cancellation even if cancel() was
                    // called before we started waiting
                    match cancel_rx.recv_timeout(Duration::from_millis(10)) {
                        Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                            if callback_state.is_cancelled() {
                                on_cancel();
                            }
                            return;
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                            // Continue polling
                        }
                    }
                }
            });

            // Spawn timeout thread if configured
            let timeout_thread = self.timeout.map(|timeout| {
                let timeout_state = self.state.clone();
                let timeout_cancel_tx = cancel_tx.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(timeout);
                    if timeout_state.cancel() {
                        // Signal callback thread
                        let _ = timeout_cancel_tx.send(());
                    }
                })
            });

            CancellationGuard {
                state,
                cancel_tx,
                _callback_thread: callback_thread,
                _timeout_thread: timeout_thread,
            }
        }
    }

    // =========================================================================
    // CancellationGuard - Active cancellation context
    // =========================================================================

    /// Guard representing an active cancellation context with callback support.
    ///
    /// Created by `CancellationSource::start()`. Use this to trigger
    /// cancellation and the registered callback will be invoked.
    pub(crate) struct CancellationGuard {
        state: Arc<SharedState>,
        cancel_tx: std::sync::mpsc::Sender<()>,
        _callback_thread: std::thread::JoinHandle<()>,
        _timeout_thread: Option<std::thread::JoinHandle<()>>,
    }

    impl CancellationGuard {
        /// Check if cancelled.
        #[inline]
        pub(crate) fn is_cancelled(&self) -> bool {
            self.state.is_cancelled()
        }

        /// Trigger cancellation.
        ///
        /// This will invoke the callback registered in `start()`.
        /// Safe to call multiple times - callback only runs once.
        pub(crate) fn cancel(&self) {
            if self.state.cancel() {
                // Signal callback thread to wake up
                let _ = self.cancel_tx.send(());
            }
        }

        /// Get a token to pass to user code.
        pub(crate) fn token(&self) -> CancellationToken {
            CancellationToken {
                state: self.state.clone(),
                timeout: None,
            }
        }
    }
}

// Re-export CancellationToken for public API
pub use cancellation::CancellationToken;
// Re-export internal types for crate-level access (when runtime integration is added)
#[allow(unused_imports)]
pub(crate) use cancellation::{CancellationGuard, CancellationSource};

/// Arguments for a BAML function call
#[derive(Default, Debug)]
pub struct FunctionArgs {
    kwargs: Vec<HostMapEntry>,
    env_overrides: Vec<HostEnvVar>,
    collectors: Vec<BamlObjectHandle>,
    type_builder: Option<BamlObjectHandle>,
    tags: Vec<HostMapEntry>,
    client_registry: Option<ClientRegistry>,
    pub(crate) cancellation_token: Option<CancellationToken>,
}

impl FunctionArgs {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_cancellation_token(
        mut self,
        cancellation_token: Option<CancellationToken>,
    ) -> Self {
        self.cancellation_token = cancellation_token;
        self
    }

    #[must_use]
    /// Add a keyword argument
    pub fn arg<V: BamlEncode>(mut self, name: &str, value: V) -> Self {
        self.kwargs.push(HostMapEntry {
            key: Some(host_map_entry::Key::StringKey(name.to_string())),
            value: Some(value.baml_encode()),
        });
        self
    }

    #[must_use]
    /// Add environment variable override
    pub fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env_overrides.push(HostEnvVar {
            key: key.to_string(),
            value: value.to_string(),
        });
        self
    }

    #[must_use]
    /// Add a tag
    pub fn with_tag<V: BamlEncode>(mut self, key: &str, value: V) -> Self {
        self.tags.push(HostMapEntry {
            key: Some(host_map_entry::Key::StringKey(key.to_string())),
            value: Some(value.baml_encode()),
        });
        self
    }

    #[must_use]
    /// Add a collector to gather telemetry
    pub fn with_collector(mut self, collector: &Collector) -> Self {
        self.collectors.push(collector.encode_handle());
        self
    }

    #[must_use]
    /// Add multiple collectors to gather telemetry
    pub fn with_collectors(mut self, collectors: &[&Collector]) -> Self {
        self.collectors
            .extend(collectors.iter().map(|c| c.encode_handle()));
        self
    }

    #[must_use]
    /// Set type builder for dynamic types
    pub fn with_type_builder(mut self, type_builder: &TypeBuilder) -> Self {
        self.type_builder = Some(type_builder.encode_handle());
        self
    }

    #[must_use]
    /// Set the client registry for runtime client configuration.
    pub fn with_client_registry(mut self, registry: &ClientRegistry) -> Self {
        self.client_registry = Some(registry.clone());
        self
    }

    /// Encode to protobuf bytes for FFI
    pub fn encode(&self) -> Result<Vec<u8>, BamlError> {
        let client_registry = self
            .client_registry
            .as_ref()
            .map(super::client_registry::ClientRegistry::encode);

        let msg = HostFunctionArguments {
            kwargs: self.kwargs.clone(),
            client_registry,
            env: self.env_overrides.clone(),
            collectors: self.collectors.clone(),
            type_builder: self.type_builder,
            tags: self.tags.clone(),
        };

        let mut buf = Vec::new();
        msg.encode(&mut buf)
            .map_err(|e| BamlError::internal(format!("failed to encode args: {e}")))?;
        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_function_args_empty() {
        let args = FunctionArgs::new();
        let encoded = args.encode();
        assert!(encoded.is_ok());
        // Empty args should still encode to something (empty protobuf message)
        let bytes = encoded.unwrap();
        assert!(bytes.is_empty() || !bytes.is_empty()); // Just check it doesn't
                                                        // panic
    }

    #[test]
    fn test_function_args_with_string() {
        let args = FunctionArgs::new().arg("text", "Hello, world!");
        let encoded = args.encode();
        assert!(encoded.is_ok());
        assert!(!encoded.unwrap().is_empty());
    }

    #[test]
    fn test_function_args_with_int() {
        let args = FunctionArgs::new().arg("count", 42i64);
        let encoded = args.encode();
        assert!(encoded.is_ok());
        assert!(!encoded.unwrap().is_empty());
    }

    #[test]
    fn test_function_args_multiple() {
        let args = FunctionArgs::new()
            .arg("name", "Alice")
            .arg("age", 30i64)
            .arg("active", true);
        let encoded = args.encode();
        assert!(encoded.is_ok());
        assert!(!encoded.unwrap().is_empty());
    }

    #[test]
    fn test_function_args_with_env() {
        let args = FunctionArgs::new()
            .arg("prompt", "test")
            .with_env("OPENAI_API_KEY", "sk-test");
        let encoded = args.encode();
        assert!(encoded.is_ok());
        assert!(!encoded.unwrap().is_empty());
    }

    #[test]
    fn test_function_args_with_tags() {
        let args = FunctionArgs::new()
            .arg("text", "hello")
            .with_tag("source", "test")
            .with_tag("priority", 1i64);
        let encoded = args.encode();
        assert!(encoded.is_ok());
        assert!(!encoded.unwrap().is_empty());
    }
}
