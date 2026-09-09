use std::sync::{Arc, Mutex};

use bridge_ctypes::{EncodedTransfer, TransferSession};

use super::super::api::BamlUnhandledSpawnErrorCallback;

/// An error and its original invocation authority travel together, including
/// while delivery waits for a host callback to register. The runtime can be
/// unavailable during final destruction; the error must still be reportable.
pub struct OwnedUnhandledSpawnError {
    pub content: EncodedTransfer<'static, Vec<u8>>,
    pub cancelled: bool,
    pub runtime: Option<Arc<dyn bex_project::Bex>>,
    pub transfers: TransferSession<'static>,
}

pub type OwnedUnhandledSpawnErrorCallback = fn(OwnedUnhandledSpawnError);

#[derive(Clone, Copy)]
enum Callback {
    Bytes(BamlUnhandledSpawnErrorCallback),
    Owned(OwnedUnhandledSpawnErrorCallback),
}

impl Callback {
    fn deliver(self, error: OwnedUnhandledSpawnError) {
        match self {
            Self::Owned(callback) => callback(error),
            Self::Bytes(callback) => {
                // Transitional C route. Remove this early commitment when the
                // remaining C consumers adopt the receipt ABI.
                let bytes = error.content.into_unreceipted();
                callback(
                    bytes.as_ptr().cast(),
                    bytes.len(),
                    i32::from(error.cancelled),
                );
            }
        }
    }
}

struct CallbackState {
    callback: Option<Callback>,
    pending: Vec<OwnedUnhandledSpawnError>,
}

struct CallbackRegistry {
    state: Mutex<CallbackState>,
}

impl CallbackRegistry {
    const fn new() -> Self {
        Self {
            state: Mutex::new(CallbackState {
                callback: None,
                pending: Vec::new(),
            }),
        }
    }

    fn register(&self, callback: Callback) {
        let pending = {
            let mut state = self
                .state
                .lock()
                .expect("unhandled spawn callback state poisoned");
            if state.callback.is_some() {
                return;
            }
            state.callback = Some(callback);
            std::mem::take(&mut state.pending)
        };
        for error in pending {
            callback.deliver(error);
        }
    }

    fn dispatch(&self, error: OwnedUnhandledSpawnError) {
        let delivery = {
            let mut state = self
                .state
                .lock()
                .expect("unhandled spawn callback state poisoned");
            match state.callback {
                Some(callback) => Some((callback, error)),
                None => {
                    state.pending.push(error);
                    None
                }
            }
        };
        if let Some((callback, error)) = delivery {
            callback.deliver(error);
        }
    }
}

static REGISTRY: CallbackRegistry = CallbackRegistry::new();

#[unsafe(no_mangle)]
pub extern "C" fn register_unhandled_spawn_error_callback(
    callback: BamlUnhandledSpawnErrorCallback,
) {
    REGISTRY.register(Callback::Bytes(callback));
}

pub fn dispatch(error: OwnedUnhandledSpawnError) {
    REGISTRY.dispatch(error);
}

/// Native bridges retain the entire error aggregate through scheduling and
/// decoding. The first registration wins across both transport forms.
pub fn register_owned_unhandled_spawn_error_callback(callback: OwnedUnhandledSpawnErrorCallback) {
    REGISTRY.register(Callback::Owned(callback));
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Barrier,
        atomic::{AtomicUsize, Ordering},
    };

    use super::{Callback, CallbackRegistry, OwnedUnhandledSpawnError};

    fn delivery(
        content: bridge_ctypes::EncodedTransfer<'static, Vec<u8>>,
    ) -> OwnedUnhandledSpawnError {
        OwnedUnhandledSpawnError {
            content,
            cancelled: false,
            runtime: None,
            transfers: bridge_ctypes::TransferSession::new(&bridge_ctypes::HANDLE_TABLE),
        }
    }

    static DELIVERED: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn count_delivery(_: *const i8, _: usize, _: i32) {
        DELIVERED.fetch_add(1, Ordering::SeqCst);
    }

    #[test]
    fn registration_dispatch_handoff_delivers_exactly_once() {
        DELIVERED.store(0, Ordering::SeqCst);
        let registry = Arc::new(CallbackRegistry::new());
        let barrier = Arc::new(Barrier::new(3));

        let register = {
            let registry = Arc::clone(&registry);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                registry.register(Callback::Bytes(count_delivery));
            })
        };
        let dispatch = {
            let registry = Arc::clone(&registry);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                registry.dispatch(delivery(
                    bridge_ctypes::OutboundEncoder::new(
                        bridge_ctypes::CffiHandleTableOptions::for_wire(),
                    )
                    .finish(vec![1, 2, 3]),
                ));
            })
        };

        barrier.wait();
        register.join().unwrap();
        dispatch.join().unwrap();

        assert_eq!(DELIVERED.load(Ordering::SeqCst), 1);
        assert!(registry.state.lock().unwrap().pending.is_empty());
    }
    fn resource_error() -> (OwnedUnhandledSpawnError, std::sync::Weak<()>) {
        let resource = Arc::new(());
        let weak = Arc::downgrade(&resource);
        let error = bex_project::UnhandledSpawnError {
            report_id: 0,
            value: bex_project::BexExternalValue::RustData(resource),
            trace: vec![],
            cancelled: false,
        };
        (
            delivery(crate::unhandled_spawn_error_to_outbound(error)),
            weak,
        )
    }

    #[test]
    fn pending_owned_error_is_retained_until_delivery_or_registry_drop() {
        let registry = CallbackRegistry::new();
        let (error, weak) = resource_error();
        registry.dispatch(error);
        assert!(weak.upgrade().is_some());
        registry.register(Callback::Owned(drop));
        assert!(weak.upgrade().is_none());
        assert!(registry.state.lock().unwrap().pending.is_empty());

        let registry = CallbackRegistry::new();
        let (error, weak) = resource_error();
        registry.dispatch(error);
        drop(registry);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn panicking_owned_handler_releases_current_and_undelivered_errors() {
        let registry = CallbackRegistry::new();
        let (first, first_weak) = resource_error();
        let (second, second_weak) = resource_error();
        registry.dispatch(first);
        registry.dispatch(second);
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            registry.register(Callback::Owned(|_error| panic!("broken native delivery")));
        }));
        assert!(outcome.is_err());
        assert!(first_weak.upgrade().is_none());
        assert!(second_weak.upgrade().is_none());
        assert!(registry.state.lock().unwrap().pending.is_empty());
    }
}
