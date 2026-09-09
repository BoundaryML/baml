//! Owned callback delivery over N-API. Reclaim rejected queue entries and
//! shutdown entries even when Node can no longer invoke JavaScript.
//!
//! napi 3.9's ThreadsafeFunction::call loses its boxed data on QueueFull, and
//! its call_js_cb returns before freeing data on a null environment. Do not
//! put lease owners into that transport. This queue follows Node's contract:
//! https://nodejs.org/api/n-api.html#asynchronous-thread-safe-function-calls
use crate::encoded_result::BamlEncodedResult;
use napi::{
    JsValue, Status,
    bindgen_prelude::{FnArgs, Function, JsValuesTupleIntoVec},
    sys,
};
use std::{
    ffi::c_void,
    ptr,
    sync::{Arc, Mutex, Weak},
};

pub(crate) type DispatchArgs = FnArgs<(u32, BamlEncodedResult)>;
pub(crate) type NotificationArgs = FnArgs<(BamlEncodedResult, bool)>;

#[derive(Clone, Copy)]
enum Delivery {
    HostCall(u32),
    UnhandledSpawn { cancelled: bool },
}
impl Delivery {
    fn failure(self, message: String) {
        match self {
            Self::HostCall(call_id) => {
                crate::host_value::send_dispatch_bridge_failure(call_id, message)
            }
            Self::UnhandledSpawn { .. } => {
                log::error!("Node unhandled-spawn delivery failed: {message}")
            }
        }
    }
}

struct State {
    raw: Mutex<sys::napi_threadsafe_function>,
}
// Only the N-API threadsafe call/release operations access this pointer off
// the JS thread. The mutex serializes calls with closing/finalization.
unsafe impl Send for State {}
unsafe impl Sync for State {}

pub(crate) struct DispatchQueue {
    state: Arc<State>,
}
struct Message {
    delivery: Delivery,
    args: Option<BamlEncodedResult>,
}

impl DispatchQueue {
    pub(crate) fn new(
        callback: Function<'_, DispatchArgs, ()>,
        capacity: usize,
    ) -> napi::Result<Self> {
        // Retain the JS callable, but not the process. Active native calls and
        // the SDK exit drain keep Node alive for actual BAML work.
        Self::create(callback, capacity, true)
    }

    pub(crate) fn notification(callback: Function<'_, NotificationArgs, ()>) -> napi::Result<Self> {
        // Errors must not compete for bounded callback slots or block the JS
        // thread. Environment teardown reclaims any undelivered envelopes.
        Self::create(callback, 0, true)
    }

    fn create<T: JsValuesTupleIntoVec>(
        callback: Function<'_, T, ()>,
        capacity: usize,
        unreferenced: bool,
    ) -> napi::Result<Self> {
        let value = callback.value();
        let mut name = ptr::null_mut();
        napi::check_status!(unsafe {
            sys::napi_create_string_utf8(value.env, c"baml_owned_dispatch".as_ptr(), 19, &mut name)
        })?;
        let state = Arc::new(State {
            raw: Mutex::new(ptr::null_mut()),
        });
        let weak = Arc::downgrade(&state).into_raw();
        let mut raw = ptr::null_mut();
        let status = unsafe {
            sys::napi_create_threadsafe_function(
                value.env,
                value.value,
                ptr::null_mut(),
                name,
                capacity,
                1,
                weak.cast_mut().cast(),
                Some(finalize),
                ptr::null_mut(),
                Some(deliver),
                &mut raw,
            )
        };
        if status != sys::Status::napi_ok {
            drop(unsafe { Weak::<State>::from_raw(weak) });
            return Err(napi::Error::from_status(status.into()));
        }
        *state
            .raw
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = raw;
        let queue = Self { state };
        if unreferenced {
            napi::check_status!(unsafe { sys::napi_unref_threadsafe_function(value.env, raw) })?;
        }
        Ok(queue)
    }

    pub(crate) fn call(&self, call_id: u32, args: BamlEncodedResult) -> Status {
        self.enqueue(Delivery::HostCall(call_id), args)
    }

    pub(crate) fn notify(&self, args: BamlEncodedResult, cancelled: bool) -> Status {
        self.enqueue(Delivery::UnhandledSpawn { cancelled }, args)
    }

    fn enqueue(&self, delivery: Delivery, args: BamlEncodedResult) -> Status {
        let mut raw = self
            .state
            .raw
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if raw.is_null() {
            return Status::Closing;
        }
        let message = Box::into_raw(Box::new(Message {
            delivery,
            args: Some(args),
        }));
        let status = unsafe {
            sys::napi_call_threadsafe_function(
                *raw,
                message.cast(),
                sys::ThreadsafeFunctionCallMode::nonblocking,
            )
        };
        // Node may deallocate after Closing. No subsequent operation, including
        // release, may use this pointer. A queued entry belongs to Node only on Ok.
        if status == sys::Status::napi_closing {
            *raw = ptr::null_mut();
        }
        drop(raw);
        if status != sys::Status::napi_ok {
            drop(unsafe { Box::from_raw(message) });
        }
        status.into()
    }
}

impl Drop for DispatchQueue {
    fn drop(&mut self) {
        let raw = std::mem::replace(
            &mut *self
                .state
                .raw
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            ptr::null_mut(),
        );
        if !raw.is_null() {
            // Finalization may run after this call; no mutex is held here.
            let _ = unsafe {
                sys::napi_release_threadsafe_function(
                    raw,
                    sys::ThreadsafeFunctionReleaseMode::release,
                )
            };
        }
    }
}

unsafe extern "C" fn finalize(_env: sys::napi_env, data: *mut c_void, _hint: *mut c_void) {
    let weak = unsafe { Weak::<State>::from_raw(data.cast()) };
    if let Some(state) = weak.upgrade() {
        *state
            .raw
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = ptr::null_mut();
    }
}

unsafe extern "C" fn deliver(
    env: sys::napi_env,
    callback: sys::napi_value,
    _ctx: *mut c_void,
    data: *mut c_void,
) {
    if data.is_null() {
        return;
    }
    // Recover ownership BEFORE testing env: Node invokes this with null env to
    // free queued entries at shutdown. Never leave lease owners in that queue.
    let mut message = unsafe { Box::<Message>::from_raw(data.cast()) };
    let delivery = message.delivery;
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> napi::Result<()> {
        if env.is_null() || callback.is_null() {
            return Err(napi::Error::from_reason(
                "Node environment closed before callback delivery",
            ));
        }
        let args = message.args.take().expect("one delivery per message");
        let abort = args.abort_guard();
        let values = match delivery {
            Delivery::HostCall(call_id) => FnArgs::from((call_id, args)).into_vec(env)?,
            Delivery::UnhandledSpawn { cancelled } => {
                FnArgs::from((args, cancelled)).into_vec(env)?
            }
        };
        let mut receiver = ptr::null_mut();
        napi::check_status!(unsafe { sys::napi_get_undefined(env, &mut receiver) })?;
        let mut returned = ptr::null_mut();
        let status = unsafe {
            sys::napi_call_function(
                env,
                receiver,
                callback,
                values.len(),
                values.as_ptr(),
                &mut returned,
            )
        };
        if status == sys::Status::napi_pending_exception {
            // A bridge handler should catch its own failures. Clear a broken
            // handler's exception and fail the waiting engine invocation.
            let mut exception = ptr::null_mut();
            let _ = unsafe { sys::napi_get_and_clear_last_exception(env, &mut exception) };
        }
        napi::check_status!(status)?;
        abort.disarm();
        Ok(())
    }));
    match outcome {
        Ok(Ok(())) => {}
        Ok(Err(error)) => delivery.failure(error.to_string()),
        Err(_) => delivery.failure("Node callback delivery panicked".into()),
    }
    // Keep destructor unwinds from crossing this C callback boundary as well.
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(message))).is_err() {
        log::error!("resource cleanup panicked after Node callback delivery");
    }
}

// Deterministic native transport probes. These enqueue on the JS thread so
// capacity cannot be consumed until the probe returns. Each message owns a
// distinct Rust resource, measured through Weak without global table counts.
#[napi_derive::napi]
pub struct DispatchQueueProbe {
    session: bridge_ctypes::TransferSession<'static>,
    resources: Vec<Weak<()>>,
    pub accepted: u32,
    pub rejected: u32,
}
#[napi_derive::napi]
impl DispatchQueueProbe {
    #[napi(getter)]
    pub fn pending(&self) -> u32 {
        self.session.pending_count() as u32
    }
    #[napi(getter)]
    pub fn retained(&self) -> u32 {
        self.resources
            .iter()
            .filter(|resource| resource.strong_count() != 0)
            .count() as u32
    }
}

#[napi_derive::napi(
    js_name = "_probeOwnedDispatch",
    ts_args_type = "callback: (callId: number, args: BamlEncodedResult) => void, count: number, capacity: number, shutdown: boolean"
)]
pub fn probe_owned_dispatch(
    callback: Function<'_, DispatchArgs, ()>,
    count: u32,
    capacity: u32,
    shutdown: bool,
) -> napi::Result<DispatchQueueProbe> {
    use bridge_ctypes::{CffiHandleTableOptions, HANDLE_TABLE, TransferSession};
    use prost::Message as _;
    let queue = DispatchQueue::new(callback, capacity as usize)?;
    let session = TransferSession::new(&HANDLE_TABLE);
    let mut probe = DispatchQueueProbe {
        session: session.clone(),
        resources: vec![],
        accepted: 0,
        rejected: 0,
    };
    for _ in 0..count {
        let resource = Arc::new(());
        probe.resources.push(Arc::downgrade(&resource));
        let encoded = bridge_ctypes::encode_to_host_call(
            &[bex_project::BexExternalValue::RustData(resource)],
            &indexmap::IndexMap::new(),
            CffiHandleTableOptions::for_wire(),
        )
        .map_err(|error| napi::Error::from_reason(error.to_string()))?
        .map_payload(|call| call.encode_to_vec());
        let args = BamlEncodedResult::new(encoded, session.clone(), None)?;
        if shutdown {
            let data = Box::into_raw(Box::new(Message {
                delivery: Delivery::HostCall(0),
                args: Some(args),
            }));
            unsafe {
                deliver(
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    data.cast(),
                );
            }
            probe.rejected += 1;
        } else if queue.call(0, args) == Status::Ok {
            probe.accepted += 1;
        } else {
            probe.rejected += 1;
        }
    }
    Ok(probe)
}
