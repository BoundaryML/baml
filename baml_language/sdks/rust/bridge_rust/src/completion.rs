//! Call-completion plumbing over the engine's callback-based C ABI.
//!
//! `call_function` is fire-and-forget: the result envelope arrives later
//! through the single registered callback, correlated by a host-side
//! dispatch id. Each in-flight call registers a [`Completion`] here; the
//! callback trampoline copies the envelope bytes and fulfills it. The
//! receiver supports both blocking (generated sync functions) and
//! `await` (generated async functions, executor-agnostic — no async
//! runtime dependency).

use std::{
    collections::HashMap,
    ffi::c_char,
    sync::{Arc, Condvar, Mutex, OnceLock},
    task::{Poll, Waker},
};

use crate::{SdkError, capi};

type CompletionResult = Result<Vec<u8>, SdkError>;

/// One in-flight call's state.
enum State {
    Pending,
    /// An async receiver parked its waker while pending.
    PendingWithWaker(Waker),
    Ready(CompletionResult),
    /// The receiver was dropped before the result arrived; the callback
    /// discards the payload.
    Abandoned,
}

struct Slot {
    state: Mutex<State>,
    ready: Condvar,
}

/// Receiver half of a registered completion. Dropping it abandons the
/// call (the eventual result is discarded, and the registry entry is
/// reclaimed by the callback or eagerly on drop).
pub(crate) struct Receiver {
    dispatch_id: u32,
    engine_call_id: u64,
    cancel_function_call: capi::CancelFunctionCallFn,
    slot: Arc<Slot>,
}

/// In-flight table: dispatch id → slot. Entries are removed when
/// fulfilled or abandoned.
fn registry() -> &'static Mutex<HashMap<u32, Arc<Slot>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<u32, Arc<Slot>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Allocate a dispatch id and register a completion for it, ensuring the
/// process-global callback is registered with the engine first (so a
/// result can never arrive unroutable).
pub(crate) fn register(api: &'static capi::Api, engine_call_id: u64) -> Receiver {
    register_with_cancel(api, engine_call_id, api.cancel_function_call)
}

fn register_with_cancel(
    api: &'static capi::Api,
    engine_call_id: u64,
    cancel_function_call: capi::CancelFunctionCallFn,
) -> Receiver {
    static CALLBACK_REGISTERED: OnceLock<()> = OnceLock::new();
    // Dispatch ids only correlate callback deliveries with waiting
    // receivers; wrap-around is harmless as long as ~4 billion calls are
    // not simultaneously in flight.
    static NEXT_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

    CALLBACK_REGISTERED.get_or_init(|| {
        // SAFETY: `trampoline` matches the engine's CallbackFn ABI and
        // never unwinds.
        #[expect(unsafe_code)]
        unsafe {
            (api.register_callback)(trampoline);
        }
    });

    let dispatch_id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let slot = Arc::new(Slot {
        state: Mutex::new(State::Pending),
        ready: Condvar::new(),
    });
    registry()
        .lock()
        .expect("completion registry poisoned")
        .insert(dispatch_id, Arc::clone(&slot));
    Receiver {
        dispatch_id,
        engine_call_id,
        cancel_function_call,
        slot,
    }
}

impl Receiver {
    pub(crate) fn dispatch_id(&self) -> u32 {
        self.dispatch_id
    }

    /// Block until the result envelope arrives.
    pub(crate) fn wait_blocking(self) -> CompletionResult {
        let mut state = self.slot.state.lock().expect("completion slot poisoned");
        loop {
            match std::mem::replace(&mut *state, State::Pending) {
                State::Ready(result) => {
                    *state = State::Abandoned;
                    drop(state);
                    return result;
                }
                other => {
                    *state = other;
                    state = self
                        .slot
                        .ready
                        .wait(state)
                        .expect("completion slot poisoned");
                }
            }
        }
    }

    /// Await the result envelope. The future is executor-agnostic.
    pub(crate) async fn wait(self) -> CompletionResult {
        struct WaitFuture(Receiver);
        impl Future for WaitFuture {
            type Output = CompletionResult;

            fn poll(
                self: std::pin::Pin<&mut Self>,
                cx: &mut std::task::Context<'_>,
            ) -> Poll<CompletionResult> {
                let mut state = self.0.slot.state.lock().expect("completion slot poisoned");
                match std::mem::replace(&mut *state, State::Pending) {
                    State::Ready(result) => {
                        *state = State::Abandoned;
                        Poll::Ready(result)
                    }
                    State::Pending | State::PendingWithWaker(_) => {
                        *state = State::PendingWithWaker(cx.waker().clone());
                        Poll::Pending
                    }
                    State::Abandoned => unreachable!("completion polled after abandonment"),
                }
            }
        }
        WaitFuture(self).await
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        // `wait_blocking` marks the slot Abandoned after taking the value;
        // an entry left in the registry means the result never arrived (or
        // the receiver is being dropped unconsumed) — reclaim it so the
        // table cannot grow without bound.
        let was_registered = registry()
            .lock()
            .expect("completion registry poisoned")
            .remove(&self.dispatch_id)
            .is_some();
        let mut state = self.slot.state.lock().expect("completion slot poisoned");
        let was_pending = matches!(*state, State::Pending | State::PendingWithWaker(_));
        *state = State::Abandoned;
        drop(state);

        // A registered, pending receiver represents a caller that stopped
        // observing the call (for example, `tokio::time::timeout` dropped its
        // future). Propagate that cancellation to the engine before the
        // caller's own concurrency permit can be reused. If the callback won
        // the race and removed the registry entry, the engine call is already
        // complete and must not be cancelled.
        if was_registered && was_pending {
            // SAFETY: the function pointer came from the validated process-
            // lifetime C API table and accepts this engine-issued call id.
            #[expect(unsafe_code)]
            unsafe {
                (self.cancel_function_call)(self.engine_call_id);
            }
        }
    }
}

/// The single engine-facing callback. Copies the envelope bytes and
/// fulfills the matching completion; results for abandoned or unknown
/// dispatch ids are discarded. Must never unwind into the engine.
extern "C" fn trampoline(call_id: u32, content: *const c_char, length: usize) {
    let caught = std::panic::catch_unwind(|| {
        let result = if length > crate::runtime::MAX_RESULT_BYTES {
            Err(SdkError::new(format!(
                "BAML result exceeded the {} MiB bridge limit (received {length} bytes)",
                crate::runtime::MAX_RESULT_BYTES / (1024 * 1024),
            )))
        } else if content.is_null() && length != 0 {
            Err(SdkError::new(
                "engine returned a null BAML result pointer with a nonzero length",
            ))
        } else {
            // SAFETY: the engine guarantees `content` is valid for `length`
            // bytes for the synchronous duration of this call.
            let bytes = if length == 0 {
                Vec::new()
            } else {
                #[expect(unsafe_code)]
                unsafe { std::slice::from_raw_parts(content.cast::<u8>(), length) }.to_vec()
            };
            Ok(bytes)
        };
        let slot = registry()
            .lock()
            .expect("completion registry poisoned")
            .remove(&call_id);
        if let Some(slot) = slot {
            let mut state = slot.state.lock().expect("completion slot poisoned");
            let previous = std::mem::replace(&mut *state, State::Ready(result));
            drop(state);
            match previous {
                State::PendingWithWaker(waker) => waker.wake(),
                State::Pending => slot.ready.notify_all(),
                // Receiver dropped between registry removal and here.
                State::Abandoned | State::Ready(_) => {}
            }
        }
    });
    if caught.is_err() {
        // A panic must not cross the C boundary; losing one completion is
        // the least-bad outcome (its receiver reports a missing result).
        // stderr because this context can neither return an error nor
        // panic, and a published SDK cannot assume a logger is installed.
        #[expect(clippy::print_stderr)]
        {
            eprintln!("baml_bridge internal error: completion callback panicked");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Register against the real loaded engine's table, but stub cancellation;
    /// these tests fulfill through the trampoline directly and never start an
    /// engine call.
    fn register() -> Receiver {
        crate::test_support::locate_dev_engine();
        register_with_cancel(
            capi::api().expect("engine library loads"),
            1,
            record_cancellation,
        )
    }

    // Fulfill directly through the trampoline, as the engine would.
    fn fulfill(id: u32, payload: &[u8]) {
        trampoline(id, payload.as_ptr().cast(), payload.len());
    }

    #[test]
    fn blocking_receive_gets_the_payload() {
        let receiver = register();
        let id = receiver.dispatch_id();
        let handle = std::thread::spawn(move || receiver.wait_blocking());
        // Give the waiter a moment to park; correctness does not depend
        // on the ordering either way.
        std::thread::yield_now();
        fulfill(id, b"hello");
        assert_eq!(handle.join().unwrap().unwrap(), b"hello");
    }

    #[test]
    fn fulfill_before_wait_is_immediate() {
        let receiver = register();
        fulfill(receiver.dispatch_id(), b"early");
        assert_eq!(receiver.wait_blocking().unwrap(), b"early");
    }

    #[test]
    fn dropped_receiver_reclaims_its_entry_and_discards_the_result() {
        let receiver = register();
        let id = receiver.dispatch_id();
        drop(receiver);
        assert!(!registry().lock().unwrap().contains_key(&id));
        // A late result for the abandoned id is discarded harmlessly.
        fulfill(id, b"late");
    }

    #[test]
    fn async_receive_gets_the_payload() {
        let receiver = register();
        let id = receiver.dispatch_id();
        let handle = std::thread::spawn(move || minimal_block_on(receiver.wait()));
        std::thread::yield_now();
        fulfill(id, b"async");
        assert_eq!(handle.join().unwrap().unwrap(), b"async");
    }

    fn fake_cancellations() -> &'static Mutex<Vec<u64>> {
        static CALLS: OnceLock<Mutex<Vec<u64>>> = OnceLock::new();
        CALLS.get_or_init(|| Mutex::new(Vec::new()))
    }

    #[expect(unsafe_code, reason = "matches the engine cancellation ABI")]
    unsafe extern "C" fn record_cancellation(call_id: u64) -> i32 {
        fake_cancellations().lock().unwrap().push(call_id);
        0
    }

    fn fake_cancel_receiver(engine_call_id: u64) -> Receiver {
        crate::test_support::locate_dev_engine();
        register_with_cancel(
            capi::api().expect("engine library loads"),
            engine_call_id,
            record_cancellation,
        )
    }

    #[test]
    fn dropping_pending_receiver_cancels_its_engine_call() {
        const ENGINE_CALL_ID: u64 = 0xCA11_CE11;
        let receiver = fake_cancel_receiver(ENGINE_CALL_ID);
        drop(receiver);
        let calls = fake_cancellations().lock().unwrap();
        assert_eq!(calls.iter().filter(|&&id| id == ENGINE_CALL_ID).count(), 1);
    }

    #[test]
    fn dropping_fulfilled_receiver_does_not_cancel() {
        const ENGINE_CALL_ID: u64 = 0xC0DE_0001;
        let receiver = fake_cancel_receiver(ENGINE_CALL_ID);
        fulfill(receiver.dispatch_id(), b"done");
        drop(receiver);
        let calls = fake_cancellations().lock().unwrap();
        assert!(!calls.contains(&ENGINE_CALL_ID));
    }

    #[test]
    fn oversized_result_is_rejected_without_reading_its_payload() {
        let receiver = register();
        trampoline(
            receiver.dispatch_id(),
            std::ptr::dangling(),
            crate::runtime::MAX_RESULT_BYTES + 1,
        );
        let error = receiver.wait_blocking().unwrap_err();
        assert!(error.to_string().contains("32 MiB bridge limit"));
    }

    /// Minimal single-future `block_on` so the test needs no async runtime.
    fn minimal_block_on<F: Future>(future: F) -> F::Output {
        use std::{
            sync::mpsc,
            task::{Context, Wake},
        };

        struct ThreadWaker(mpsc::Sender<()>);
        impl Wake for ThreadWaker {
            fn wake(self: Arc<Self>) {
                let _ = self.0.send(());
            }
        }

        let (tx, rx) = mpsc::channel();
        let waker = std::task::Waker::from(Arc::new(ThreadWaker(tx)));
        let mut cx = Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);
        loop {
            match future.as_mut().poll(&mut cx) {
                Poll::Ready(value) => return value,
                Poll::Pending => {
                    rx.recv().expect("waker channel closed");
                }
            }
        }
    }
}
