use std::sync::Mutex;

use super::super::api::BamlUnhandledSpawnErrorCallback;

type PendingError = (Vec<u8>, bool);

struct CallbackState {
    callback: Option<BamlUnhandledSpawnErrorCallback>,
    pending: Vec<PendingError>,
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

    fn register(&self, callback: BamlUnhandledSpawnErrorCallback) {
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
        for (content, cancelled) in pending {
            callback(content.as_ptr().cast(), content.len(), i32::from(cancelled));
        }
    }

    fn dispatch(&self, content: Vec<u8>, cancelled: bool) {
        let delivery = {
            let mut state = self
                .state
                .lock()
                .expect("unhandled spawn callback state poisoned");
            match state.callback {
                Some(callback) => Some((callback, content, cancelled)),
                None => {
                    state.pending.push((content, cancelled));
                    None
                }
            }
        };
        if let Some((callback, content, cancelled)) = delivery {
            callback(content.as_ptr().cast(), content.len(), i32::from(cancelled));
        }
    }
}

static REGISTRY: CallbackRegistry = CallbackRegistry::new();

#[unsafe(no_mangle)]
pub extern "C" fn register_unhandled_spawn_error_callback(
    callback: BamlUnhandledSpawnErrorCallback,
) {
    REGISTRY.register(callback);
}

pub fn dispatch(content: Vec<u8>, cancelled: bool) {
    REGISTRY.dispatch(content, cancelled);
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Barrier,
        atomic::{AtomicUsize, Ordering},
    };

    use super::CallbackRegistry;

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
                registry.register(count_delivery);
            })
        };
        let dispatch = {
            let registry = Arc::clone(&registry);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                registry.dispatch(vec![1, 2, 3], false);
            })
        };

        barrier.wait();
        register.join().unwrap();
        dispatch.join().unwrap();

        assert_eq!(DELIVERED.load(Ordering::SeqCst), 1);
        assert!(registry.state.lock().unwrap().pending.is_empty());
    }
}
