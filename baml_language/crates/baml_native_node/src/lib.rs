//! A Node addon that calls a BAML program compiled to a native library,
//! built on the core layer of `baml_native_host`
//! (`baml_native_rt/design/bridges.md`, "Node (built)").
//!
//! The JavaScript runtime in `js/` drives it:
//!
//! - `load(path, hash, onWake)` loads the library and checks its interface
//!   hash. `onWake` is called on Node's loop, through a thread-safe
//!   function, when the library wants a turn (from its I/O thread). The
//!   thread-safe function keeps the loop alive only while calls are
//!   pending.
//! - `bind(constructors, factories, enumValues, makeMap, mode)` turns every
//!   exported type into a plan, given the generated class and enum for each
//!   interface type.
//! - `start(export, args)` builds the arguments by their declared types and
//!   starts the call; `call(export, args)` does the same and runs a turn at
//!   once. `turn()` runs a turn; `drive(id)` runs turns until call `id`
//!   ended, sleeping on the library's timer or wake in between (the `_sync`
//!   functions).
//! - A turn copies each result on the spot, in this one native call, by its
//!   export's plan: the plan reads the value through the core and builds the
//!   JavaScript value directly. Every turn returns
//!   `[delayMs, pending, id, status, payload, ...]`.
//!
//! Every entry point runs on Node's main thread. One that is entered while
//! another holds the library (a constructor called during a copy that calls
//! a binding) throws instead of waiting for the lock.

#![expect(
    unsafe_code,
    reason = "the Node-API C functions, and the addon's entry points Node calls"
)]

mod copy;
mod js;

use std::{
    cell::{Cell, RefCell},
    ffi::c_void,
    ptr,
    rc::Rc,
    sync::{
        Condvar, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicPtr, Ordering},
    },
    time::Duration,
};

use baml_native_host::{Event, Library, Outcome, Session};
use napi_sys as sys;

use crate::{
    copy::{CopyError, Plans},
    js::{Env, Value},
};

/// How a call ended, as a turn reports it to JavaScript.
mod status {
    pub(crate) const RETURNED: u32 = 0;
    pub(crate) const THREW: u32 = 1;
    pub(crate) const STUCK: u32 = 2;
    pub(crate) const PANICKED: u32 = 3;
    /// A task the call spawned failed and nothing awaits it.
    pub(crate) const UNHANDLED: u32 = 4;
    /// Copying the result threw in JavaScript (a constructor); the payload
    /// is what it threw.
    pub(crate) const HOST_THREW: u32 = 5;
    /// The result did not have the shape its type says; the payload is why.
    pub(crate) const DECODE: u32 = 6;
}

/// What any thread may touch: the wake path.
struct Shared {
    /// The thread-safe function that calls `onWake` on Node's loop.
    tsfn: AtomicPtr<c_void>,
    /// A wake is posted to the loop and its turn has not started yet.
    wake_posted: AtomicBool,
    /// Set by every wake; what `drive` sleeps on.
    woken: Mutex<bool>,
    condvar: Condvar,
}

static SHARED: Shared = Shared {
    tsfn: AtomicPtr::new(ptr::null_mut()),
    wake_posted: AtomicBool::new(false),
    woken: Mutex::new(false),
    condvar: Condvar::new(),
};

static LIBRARY: OnceLock<&'static Library> = OnceLock::new();

/// What only Node's main thread touches.
#[derive(Default)]
struct Main {
    plans: Option<Rc<Plans>>,
    /// Calls started and not ended. The thread-safe function is referenced
    /// (keeps the loop alive) exactly while this is not zero.
    pending: u32,
}

thread_local! {
    static MAIN: RefCell<Main> = RefCell::new(Main::default());
    /// An entry point holds the library.
    static BUSY: Cell<bool> = const { Cell::new(false) };
}

/// Called by the library, from any thread, when it wants a turn.
fn wake() {
    {
        let mut woken = SHARED
            .woken
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *woken = true;
    }
    SHARED.condvar.notify_all();
    if !SHARED.wake_posted.swap(true, Ordering::AcqRel) {
        let tsfn = SHARED.tsfn.load(Ordering::Acquire);
        if !tsfn.is_null() {
            // SAFETY: a thread-safe function, callable from any thread.
            unsafe {
                sys::napi_call_threadsafe_function(
                    tsfn.cast(),
                    ptr::null_mut(),
                    sys::ThreadsafeFunctionCallMode::nonblocking,
                )
            };
        }
    }
}

/// On Node's loop, for each wake posted: call `onWake`.
unsafe extern "C" fn call_on_wake(
    env: Env,
    on_wake: Value,
    _context: *mut c_void,
    _data: *mut c_void,
) {
    if env.is_null() || on_wake.is_null() {
        return;
    }
    if js::call(env, on_wake, &[]).is_err() {
        // `onWake` only schedules a turn; what it threw is not the call's.
    }
}

/// Holding the library from an entry point; refuses re-entry.
struct Busy;

impl Busy {
    fn enter(env: Env) -> Result<Self, Value> {
        if BUSY.with(|busy| busy.replace(true)) {
            return Err(js::error(
                env,
                Some("reentrant"),
                "a BAML function was called while the native bridge was copying a value into \
                 JavaScript (from a class constructor?): call it after the result is returned",
            ));
        }
        Ok(Self)
    }
}

impl Drop for Busy {
    fn drop(&mut self) {
        BUSY.with(|busy| busy.set(false));
    }
}

fn library(env: Env) -> Result<&'static Library, Value> {
    LIBRARY
        .get()
        .copied()
        .ok_or_else(|| js::error(env, None, "the BAML library is not loaded"))
}

fn plans(env: Env) -> Result<Rc<Plans>, Value> {
    MAIN.with(|main| main.borrow().plans.clone())
        .ok_or_else(|| js::error(env, None, "the BAML library's types are not bound"))
}

/// Run an entry point: its `Err` is thrown.
fn entry(env: Env, body: impl FnOnce() -> Result<Value, Value>) -> Value {
    match body() {
        Ok(value) => value,
        Err(error) => js::throw(env, error),
    }
}

/// Call and pending-count bookkeeping when a call started.
fn started(env: Env) {
    MAIN.with(|main| {
        let mut main = main.borrow_mut();
        main.pending += 1;
        if main.pending == 1 {
            let tsfn = SHARED.tsfn.load(Ordering::Acquire);
            if !tsfn.is_null() {
                // SAFETY: the thread-safe function, on the main thread.
                unsafe { sys::napi_ref_threadsafe_function(env, tsfn.cast()) };
            }
        }
    });
}

fn ended(env: Env) {
    MAIN.with(|main| {
        let mut main = main.borrow_mut();
        main.pending = main.pending.saturating_sub(1);
        if main.pending == 0 {
            let tsfn = SHARED.tsfn.load(Ordering::Acquire);
            if !tsfn.is_null() {
                // SAFETY: as above.
                unsafe { sys::napi_unref_threadsafe_function(env, tsfn.cast()) };
            }
        }
    });
}

fn pending() -> u32 {
    MAIN.with(|main| main.borrow().pending)
}

/// The events of turns, as they are handed to JavaScript.
struct Events {
    array: Value,
    next: u32,
}

impl Events {
    fn new(env: Env) -> Self {
        Self {
            array: js::array(env, 2),
            next: 2,
        }
    }

    fn push(&mut self, env: Env, call: u64, status: u32, payload: Value) {
        #[expect(clippy::cast_precision_loss, reason = "call ids stay far below 2^53")]
        let id = js::number(env, call as f64);
        let _ = js::set_element(env, self.array, self.next, id);
        let _ = js::set_element(
            env,
            self.array,
            self.next + 1,
            js::number(env, f64::from(status)),
        );
        let _ = js::set_element(env, self.array, self.next + 2, payload);
        self.next += 3;
    }

    fn finish(self, env: Env, delay: Option<Duration>) -> Value {
        let delay = delay.map_or(-1.0, |delay| delay.as_secs_f64() * 1e3);
        let _ = js::set_element(env, self.array, 0, js::number(env, delay));
        let _ = js::set_element(env, self.array, 1, js::number(env, f64::from(pending())));
        self.array
    }
}

/// One turn: every event is copied into JavaScript on the spot and added to
/// `events`. Returns the delay until the next BAML timer, and whether call
/// `want` ended in it.
fn run_turn(
    env: Env,
    session: &Session,
    plans: &Plans,
    events: &mut Events,
    want: Option<u64>,
) -> (Option<Duration>, bool) {
    SHARED.wake_posted.store(false, Ordering::Release);
    let mut found = false;
    let delay = session.turn(|event| match event {
        Event::Ended {
            call,
            outcome,
            value,
        } => {
            ended(env);
            found |= want == Some(call);
            let (status, payload) = match outcome {
                Outcome::Returned => match plans.result(env, call_export(call), &value) {
                    Ok(js_value) => (status::RETURNED, js_value),
                    Err(CopyError::Js(thrown)) => (status::HOST_THREW, thrown),
                    Err(CopyError::Decode(why)) => (status::DECODE, js::string(env, &why)),
                },
                Outcome::Threw => (status::THREW, plans.thrown(env, &value)),
                Outcome::Stuck => (status::STUCK, js::undefined(env)),
                Outcome::Panicked => (status::PANICKED, plans.thrown(env, &value)),
            };
            forget_call(call);
            events.push(env, call, status, payload);
        }
        Event::Unhandled { call, error } => {
            events.push(env, call, status::UNHANDLED, plans.thrown(env, &error));
        }
    });
    (delay, found)
}

thread_local! {
    /// The export of each pending call, by call id: a turn needs it to pick
    /// the result's plan.
    static CALLS: RefCell<std::collections::HashMap<u64, u32>> = RefCell::default();
}

fn call_export(call: u64) -> u32 {
    CALLS.with(|calls| calls.borrow().get(&call).copied().unwrap_or(u32::MAX))
}

fn forget_call(call: u64) {
    CALLS.with(|calls| calls.borrow_mut().remove(&call));
}

/// Build the arguments and start export `export`; its call id.
fn start_call(
    env: Env,
    session: &Session,
    plans: &Plans,
    export: Value,
    args: Value,
) -> Result<u64, Value> {
    let export = u32::try_from(js::get_f64(env, export) as i64)
        .map_err(|_| js::error(env, None, "no such export"))?;
    let built = plans.arguments(env, session, export, args)?;
    let call = session
        .start(export, &built)
        .map_err(|error| js::error(env, Some("invalid_argument"), &error.to_string()))?;
    drop(built);
    CALLS.with(|calls| calls.borrow_mut().insert(call, export));
    started(env);
    Ok(call)
}

unsafe extern "C" fn load(env: Env, info: sys::napi_callback_info) -> Value {
    let [path, hash, on_wake] = js::arguments::<3>(env, info);
    entry(env, || {
        let mut buffer = Vec::new();
        let path = js::get_string(env, path, &mut buffer).to_owned();
        let hash_text = js::get_string(env, hash, &mut buffer).to_owned();
        let expected = u64::from_str_radix(hash_text.trim_start_matches("0x"), 16)
            .map_err(|_| js::error(env, None, &format!("not an interface hash: {hash_text}")))?;
        if SHARED.tsfn.load(Ordering::Acquire).is_null() {
            let mut tsfn = ptr::null_mut();
            // SAFETY: `on_wake` is a function of this call; the thread-safe
            // function is made unreferenced, and referenced while calls are
            // pending.
            unsafe {
                sys::napi_create_threadsafe_function(
                    env,
                    on_wake,
                    ptr::null_mut(),
                    js::string(env, "baml-native-wake"),
                    0,
                    1,
                    ptr::null_mut(),
                    None,
                    ptr::null_mut(),
                    Some(call_on_wake),
                    &raw mut tsfn,
                );
                sys::napi_unref_threadsafe_function(env, tsfn);
            }
            SHARED.tsfn.store(tsfn.cast(), Ordering::Release);
        }
        let library = Library::load(&path, expected, wake)
            .map_err(|error| js::error(env, None, &error.to_string()))?;
        let _ = LIBRARY.set(library);
        Ok(js::string(env, library.interface_json()))
    })
}

/// `types()`: per interface type, `[name, isEnum, fieldOrVariantNames]`.
unsafe extern "C" fn types(env: Env, _info: sys::napi_callback_info) -> Value {
    entry(env, || Ok(copy::describe_types(env, library(env)?)))
}

/// `functions()`: per export, `[name, paramNames, hasDefault]`.
unsafe extern "C" fn functions(env: Env, _info: sys::napi_callback_info) -> Value {
    entry(env, || Ok(copy::describe_functions(env, library(env)?)))
}

unsafe extern "C" fn bind(env: Env, info: sys::napi_callback_info) -> Value {
    let [constructors, factories, enum_values, make_map, mode] = js::arguments::<5>(env, info);
    entry(env, || {
        let library = library(env)?;
        let mode = js::get_f64(env, mode) as u32;
        let plans = Plans::build(
            env,
            library,
            constructors,
            factories,
            enum_values,
            make_map,
            mode,
        )
        .map_err(|thrown| thrown.0)?;
        MAIN.with(|main| main.borrow_mut().plans = Some(Rc::new(plans)));
        Ok(js::undefined(env))
    })
}

/// `start(export, args)`: the call id.
unsafe extern "C" fn start(env: Env, info: sys::napi_callback_info) -> Value {
    let [export, args] = js::arguments::<2>(env, info);
    entry(env, || {
        let _busy = Busy::enter(env)?;
        let plans = plans(env)?;
        let session = library(env)?.session();
        let call = start_call(env, &session, &plans, export, args)?;
        #[expect(clippy::cast_precision_loss, reason = "call ids stay far below 2^53")]
        Ok(js::number(env, call as f64))
    })
}

/// `call(export, args)`: starts the call and runs a turn at once. The turn's
/// events, with the call id in front: `[id, delayMs, pending, ...]`.
unsafe extern "C" fn call(env: Env, info: sys::napi_callback_info) -> Value {
    let [export, args] = js::arguments::<2>(env, info);
    entry(env, || {
        let _busy = Busy::enter(env)?;
        let plans = plans(env)?;
        let session = library(env)?.session();
        let call = start_call(env, &session, &plans, export, args)?;
        let mut events = Events::new(env);
        let (delay, _) = run_turn(env, &session, &plans, &mut events, Some(call));
        drop(session);
        let array = events.finish(env, delay);
        let out = js::array(env, 2);
        #[expect(clippy::cast_precision_loss, reason = "call ids stay far below 2^53")]
        let _ = js::set_element(env, out, 0, js::number(env, call as f64));
        let _ = js::set_element(env, out, 1, array);
        Ok(out)
    })
}

/// `turn()`: `[delayMs, pending, id, status, payload, ...]`.
unsafe extern "C" fn turn(env: Env, _info: sys::napi_callback_info) -> Value {
    entry(env, || {
        let _busy = Busy::enter(env)?;
        let plans = plans(env)?;
        let session = library(env)?.session();
        let mut events = Events::new(env);
        let (delay, _) = run_turn(env, &session, &plans, &mut events, None);
        drop(session);
        Ok(events.finish(env, delay))
    })
}

/// `drive(id)`: turns until call `id` ended, sleeping on the library's
/// timer or wake in between. Every event of those turns, as `turn()`
/// returns them.
unsafe extern "C" fn drive(env: Env, info: sys::napi_callback_info) -> Value {
    let [id] = js::arguments::<1>(env, info);
    entry(env, || {
        let _busy = Busy::enter(env)?;
        let plans = plans(env)?;
        let library = library(env)?;
        let want = js::get_f64(env, id) as u64;
        let mut events = Events::new(env);
        loop {
            *SHARED
                .woken
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = false;
            let session = library.session();
            let (delay, found) = run_turn(env, &session, &plans, &mut events, Some(want));
            drop(session);
            if found {
                return Ok(events.finish(env, delay));
            }
            let woken = SHARED
                .woken
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match delay {
                Some(delay) => {
                    drop(
                        SHARED
                            .condvar
                            .wait_timeout_while(woken, delay, |woken| !*woken),
                    );
                }
                None => {
                    drop(SHARED.condvar.wait_while(woken, |woken| !*woken));
                }
            }
        }
    })
}

/// `handleCount()`: live handles. For tests.
unsafe extern "C" fn handle_count(env: Env, _info: sys::napi_callback_info) -> Value {
    entry(env, || {
        let _busy = Busy::enter(env)?;
        #[expect(clippy::cast_precision_loss, reason = "a count")]
        Ok(js::number(env, library(env)?.handle_count() as f64))
    })
}

/// `collect()`: collect now. For tests.
unsafe extern "C" fn collect(env: Env, _info: sys::napi_callback_info) -> Value {
    entry(env, || {
        let _busy = Busy::enter(env)?;
        library(env)?.collect();
        Ok(js::undefined(env))
    })
}

/// The Node-API version the addon is written for: 10 lets it keep references
/// to strings (field names, enum members), which version 8 refuses.
#[unsafe(no_mangle)]
pub extern "C" fn node_api_module_get_api_version_v1() -> i32 {
    10
}

/// Node's entry point for the addon.
///
/// # Safety
///
/// Called by Node with the env and exports object of the addon's module.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_register_module_v1(env: Env, exports: Value) -> Value {
    js::export(env, exports, c"load", load);
    js::export(env, exports, c"types", types);
    js::export(env, exports, c"functions", functions);
    js::export(env, exports, c"bind", bind);
    js::export(env, exports, c"start", start);
    js::export(env, exports, c"call", call);
    js::export(env, exports, c"turn", turn);
    js::export(env, exports, c"drive", drive);
    js::export(env, exports, c"handleCount", handle_count);
    js::export(env, exports, c"collect", collect);
    exports
}
