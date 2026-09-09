//! Coalesced, acknowledged host-registration releases. A wakeup carries no
//! per-release allocation; keys remain here until the JS registry accepts a
//! batch. The channel is unreferenced so it cannot keep Node alive at exit.
use crate::handle::HandleKey;
use napi::{
    JsValue, Status,
    bindgen_prelude::{Function, ToNapiValue},
    sys,
};
use std::{
    collections::HashSet,
    ffi::c_void,
    ptr,
    sync::{Arc, Mutex},
};

const BATCH_SIZE: usize = 1024;

struct Inner {
    raw: sys::napi_threadsafe_function,
    pending: HashSet<u64>,
    scheduled: bool,
    closed: bool,
    wakes: u32,
}
struct State {
    inner: Mutex<Inner>,
}
// Only N-API's threadsafe enqueue/release operations use raw off the JS
// thread. The mutex serializes them with closing and finalization.
unsafe impl Send for State {}
unsafe impl Sync for State {}

pub(crate) struct ReleaseQueue {
    state: Arc<State>,
}

impl ReleaseQueue {
    pub(crate) fn new(callback: Function<'_, Vec<HandleKey>, ()>) -> napi::Result<Self> {
        let value = callback.value();
        let mut name = ptr::null_mut();
        napi::check_status!(unsafe {
            sys::napi_create_string_utf8(value.env, c"baml_host_releases".as_ptr(), 18, &mut name)
        })?;
        let state = Arc::new(State {
            inner: Mutex::new(Inner {
                raw: ptr::null_mut(),
                pending: HashSet::new(),
                scheduled: false,
                closed: false,
                wakes: 0,
            }),
        });
        let context = Arc::into_raw(state.clone());
        let mut raw = ptr::null_mut();
        let status = unsafe {
            sys::napi_create_threadsafe_function(
                value.env,
                value.value,
                ptr::null_mut(),
                name,
                // There is at most one queued wakeup. Use zero capacity to rule
                // out QueueFull rather than making cleanup compete for slots.
                0,
                1,
                context.cast_mut().cast(),
                Some(finalize),
                context.cast_mut().cast(),
                Some(deliver),
                &mut raw,
            )
        };
        if status != sys::Status::napi_ok {
            drop(unsafe { Arc::<State>::from_raw(context) });
            return Err(napi::Error::from_status(status.into()));
        }
        state
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .raw = raw;
        let queue = Self { state };
        napi::check_status!(unsafe { sys::napi_unref_threadsafe_function(value.env, raw) })?;
        Ok(queue)
    }

    pub(crate) fn enqueue(&self, key: u64) -> Status {
        let mut inner = self
            .state
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if inner.closed || inner.raw.is_null() {
            return Status::Closing;
        }
        inner.pending.insert(key);
        wake(&mut inner)
    }

    fn close(&self) {
        let raw = {
            let mut inner = self
                .state
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            inner.closed = true;
            inner.scheduled = false;
            inner.pending = HashSet::new();
            std::mem::replace(&mut inner.raw, ptr::null_mut())
        };
        if !raw.is_null() {
            let _ = unsafe {
                sys::napi_release_threadsafe_function(
                    raw,
                    sys::ThreadsafeFunctionReleaseMode::abort,
                )
            };
        }
    }
}
impl Drop for ReleaseQueue {
    fn drop(&mut self) {
        self.close();
    }
}

fn wake(inner: &mut Inner) -> Status {
    if inner.scheduled || inner.pending.is_empty() {
        return Status::Ok;
    }
    let status = unsafe {
        sys::napi_call_threadsafe_function(
            inner.raw,
            ptr::null_mut(),
            sys::ThreadsafeFunctionCallMode::nonblocking,
        )
    };
    if status == sys::Status::napi_ok {
        inner.scheduled = true;
        inner.wakes = inner.wakes.saturating_add(1);
    } else if status == sys::Status::napi_closing {
        // Node owns environment teardown now; never access this pointer again.
        // JS registrations are destroyed with that environment, not retained
        // in a process-global Rust list of undeliverable release keys.
        inner.raw = ptr::null_mut();
        inner.closed = true;
        inner.pending = HashSet::new();
    }
    // Unexpected enqueue errors preserve every key for retry; do not silently
    // acknowledge a failed delivery. The valid unbounded N-API channel cannot
    // return QueueFull. A future enqueue can retry an infrastructure failure.
    status.into()
}

unsafe extern "C" fn finalize(_env: sys::napi_env, data: *mut c_void, _hint: *mut c_void) {
    let state = unsafe { Arc::<State>::from_raw(data.cast()) };
    let mut inner = state
        .inner
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    inner.raw = ptr::null_mut();
    inner.closed = true;
    inner.scheduled = false;
    inner.pending = HashSet::new();
}

unsafe extern "C" fn deliver(
    env: sys::napi_env,
    callback: sys::napi_value,
    context: *mut c_void,
    _data: *mut c_void,
) {
    // N-API holds this context Arc until all queued callbacks finish.
    let state = unsafe { &*context.cast::<State>() };
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> napi::Result<()> {
        let keys = {
            let mut inner = state
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if env.is_null() || callback.is_null() {
                inner.raw = ptr::null_mut();
                inner.closed = true;
            }
            if inner.closed {
                inner.scheduled = false;
                inner.pending = HashSet::new();
                return Ok(());
            }
            inner
                .pending
                .iter()
                .take(BATCH_SIZE)
                .copied()
                .collect::<Vec<_>>()
        };
        // Leave keys pending until the JS callback returns successfully. It
        // only deletes registry entries, so repeating a partial batch is safe.
        let value = unsafe {
            Vec::<HandleKey>::to_napi_value(
                env,
                keys.iter().copied().map(HandleKey::from_u64).collect(),
            )
        }?;
        let mut receiver = ptr::null_mut();
        napi::check_status!(unsafe { sys::napi_get_undefined(env, &mut receiver) })?;
        let mut returned = ptr::null_mut();
        let status =
            unsafe { sys::napi_call_function(env, receiver, callback, 1, &value, &mut returned) };
        if status == sys::Status::napi_pending_exception {
            let mut exception = ptr::null_mut();
            let _ = unsafe { sys::napi_get_and_clear_last_exception(env, &mut exception) };
        }
        napi::check_status!(status)?;
        let mut inner = state
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for key in keys {
            inner.pending.remove(&key);
        }
        if inner.pending.is_empty() {
            inner.pending = HashSet::new();
        }
        Ok(())
    }));
    if !matches!(outcome, Ok(Ok(()))) {
        log::error!("Node host-release delivery failed; retaining keys for retry");
    }
    // Enqueues during delivery see scheduled=true and simply join pending.
    // Yield between batches so cleanup doesn't monopolize the event loop.
    let mut inner = state
        .inner
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    inner.scheduled = false;
    if !inner.closed && !inner.raw.is_null() {
        let status = wake(&mut inner);
        if status != Status::Ok && status != Status::Closing {
            log::error!("Node host-release wakeup failed: {status:?}; keys retained");
        }
    }
}

#[napi_derive::napi(object)]
pub struct ReleaseQueueStats {
    pub pending: u32,
    pub capacity: u32,
    pub scheduled: bool,
    pub closed: bool,
    pub wakes: u32,
}
impl ReleaseQueue {
    pub(crate) fn stats(&self) -> ReleaseQueueStats {
        let inner = self
            .state
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ReleaseQueueStats {
            pending: inner.pending.len() as u32,
            capacity: inner.pending.capacity() as u32,
            scheduled: inner.scheduled,
            closed: inner.closed,
            wakes: inner.wakes,
        }
    }
}

/// Isolated native channel for deterministic retry/reentrancy/close tests.
#[napi_derive::napi]
pub struct ReleaseQueueProbe {
    queue: ReleaseQueue,
}
#[napi_derive::napi]
impl ReleaseQueueProbe {
    #[napi]
    pub fn enqueue(&self, keys: Vec<HandleKey>) {
        for key in keys {
            let _ = self.queue.enqueue(key.to_u64());
        }
    }
    #[napi(getter)]
    pub fn stats(&self) -> ReleaseQueueStats {
        self.queue.stats()
    }
    #[napi]
    pub fn close(&self) {
        self.queue.close();
    }
}
#[napi_derive::napi(
    js_name = "_probeReleaseQueue",
    ts_args_type = "callback: (keys: Array<HandleKey>) => void"
)]
pub fn probe_release_queue(
    callback: Function<'_, Vec<HandleKey>, ()>,
) -> napi::Result<ReleaseQueueProbe> {
    Ok(ReleaseQueueProbe {
        queue: ReleaseQueue::new(callback)?,
    })
}
