//! The library form: a compiled program a host program loads and calls.
//!
//! The program is linked as a shared library with no `main` and no thread
//! of its own. The host starts calls and gives the worker turns, on its
//! own thread (`design/bridges.md`):
//!
//! ```text
//! baml_lib_init(&callbacks)              build the worker
//! baml_lib_call_start(fn, args, n)       a task for the call, queued; a call id
//! baml_lib_turn()                        run tasks until each is done or parked
//!    -> on_done(ctx, call, status, tag, payload)    each call that ended
//!    -> on_unhandled(ctx, call, tag, payload)       each failure nobody awaits
//!    -> wake(ctx)                                   from the I/O thread
//! ```
//!
//! A turn never sleeps: where the executable's worker waits on its inbox,
//! a turn returns, and the I/O thread that fills the inbox also calls the
//! host's `wake`. A task that sleeps on a timer is the worker's own: a turn
//! returns how long until the next one fires, and the host turns again
//! then.
//!
//! # Callbacks
//!
//! - `wake` is called from any thread, the I/O thread most often.
//! - `on_done` and `on_unhandled` are called only inside `baml_lib_turn`,
//!   on the thread that called it, after BAML has stopped running, on that
//!   thread's own stack.
//! - A callback must not call into the library. The host owns every handle
//!   a callback hands it and must free it.
//!
//! `on_done` says how a call ended: [`STATUS_RETURNED`] with its value,
//! [`STATUS_THREW`] with what it threw, [`STATUS_PANICKED`] with the
//! `baml.panics` instance it panicked with (a `baml.panics.Exit` too, which
//! never ends the host's process), or [`STATUS_STUCK`] with `null` when it
//! can never finish: nothing can run, no timer is set, and no I/O can
//! complete. After `STATUS_STUCK` the call is over for the host; if its
//! task ever finished, its result would be dropped.
//!
//! `on_unhandled` hands over the error of a task the call spawned that
//! failed and that nothing awaited, once that call has finished: then
//! nothing can await it any more. The task is freed.
//!
//! # Values
//!
//! A value crosses as its two words. A scalar is what a slot holds. An
//! object keeps its own tag, with [`HANDLE_BIT`] set: its payload is then a
//! handle, the number of a slot of the handle table, which the collector
//! keeps current, never an address. A host holds handles across turns and
//! collections; it reads objects only through the accessors here, which
//! copy.
//!
//! # What must hold
//!
//! One worker per process, and one thread in this module at a time: no
//! call here may run during a turn, nor two at once. Only `wake` comes from
//! another thread. The turn runs on the host's stack, which is the worker's
//! own stack for its duration.

mod table;
mod types;
pub mod values;

use std::ffi::c_void;

pub(crate) use table::each_slot;

use super::{
    DEAD, NOTIFIED, PARKED, PARKING, RUNNABLE, RUNNING, TASK_FAILED, TASK_STACK, Task, WORKER,
    baml_rt_resume, load, store,
};
use crate::{
    heap::{Mutator, Program, Value},
    layout::{
        ARRAY_PTR, FUTURE_FLAGS, FUTURE_OBSERVED, FUTURE_TASK, FUTURE_VALUE, HEADER_PANIC,
        STORAGE_ELEMENTS, STRING_TAGS_END, TAG_INSTANCE, TAG_INT, TAG_STRING, THROWN, VALUE,
    },
};

/// Set in the tag of a value that crosses to or from the host when its
/// payload is a handle. Bit 61 is free in every tag and header
/// (`layout.rs`), and a value with it is never stored on the heap.
pub const HANDLE_BIT: usize = 1 << 61;

/// The version of this interface: of the callbacks, the statuses and the
/// functions. A host checks it with [`baml_lib_abi_version`].
pub const ABI_VERSION: u32 = 2;

/// How a call ended, as `on_done` says.
pub const STATUS_RETURNED: u32 = 0;
pub const STATUS_THREW: u32 = 1;
pub const STATUS_STUCK: u32 = 2;
pub const STATUS_PANICKED: u32 = 3;

/// What the host gives `baml_lib_init`. `size` is the size of the struct
/// the host knows, so that a later version can add fields.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Callbacks {
    pub size: u32,
    pub ctx: *mut c_void,
    pub on_done:
        unsafe extern "C" fn(ctx: *mut c_void, call: u64, status: u32, tag: u64, payload: u64),
    pub on_unhandled: unsafe extern "C" fn(ctx: *mut c_void, call: u64, tag: u64, payload: u64),
    pub wake: unsafe extern "C" fn(ctx: *mut c_void),
}

unsafe extern "C" {
    /// Defined by the compiled library: what a task started by the host
    /// starts in. It asks for its entry and arguments
    /// ([`baml_rt_host_start`]), calls the entry and reports the result
    /// ([`baml_rt_host_finish`]).
    fn baml_native_host_shim(mutator: *mut c_void);
    /// Defined by the compiled library: what a task BAML spawns starts in.
    fn baml_native_task_shim(mutator: *mut c_void);
    /// Defined by the compiled library: its exported functions.
    static baml_native_exports: u8;
    static baml_native_stack_maps: usize;
    static baml_native_types: usize;
    static baml_native_unwind: usize;
    static baml_native_functions: usize;
}

/// One call in flight: what all its tasks share. A task's `wait_value` is
/// the number of its call's record, which every task it spawns inherits.
/// A record is free again once the call's last task has ended and its
/// failures are reported.
#[derive(Default)]
struct CallRecord {
    id: u64,
    /// How many of its tasks have not ended.
    live: u32,
    /// How many of them failed and have not been reported.
    failed: u32,
    /// Its first task has returned or thrown: `on_done` has it.
    root_done: bool,
    /// It was reported stuck: what it returns later is dropped.
    abandoned: bool,
}

/// A call that ended in this turn, for `on_done` after it.
struct Ended {
    call: u64,
    status: u32,
    value: Value,
}

struct Host {
    /// The worker, made once and never freed: compiled code reaches it
    /// through its pinned register while a turn runs, so no Rust reference
    /// to it outlives a call of this module.
    worker: *mut Mutator,
    /// The library's functions, classes and enums, read at init.
    interface: types::Interface,
    /// Calls that ended in this turn, reported after it, and the vector
    /// they are swapped with so that neither is made again each turn.
    ended: Vec<Ended>,
    reporting: Vec<Ended>,
    next_call: u64,
    callbacks: Callbacks,
    /// The calls in flight, by record number, and the free records.
    calls: Vec<CallRecord>,
    free_calls: Vec<usize>,
    /// Records whose failures are to be reported at the end of the turn:
    /// their last task ended, or they were found stuck.
    to_report: Vec<usize>,
    /// Why the last call that refused something refused it.
    error: String,
}

impl Host {
    fn worker(&mut self) -> &mut Mutator {
        // SAFETY: made at init and never freed; see the field.
        unsafe { &mut *self.worker }
    }
}

/// Keep `why` for `baml_lib_last_error`, and return `value`: what the
/// refusing function returns.
fn refuse<T>(why: String, value: T) -> T {
    // SAFETY: a short write of the host's state.
    unsafe { host().error = why };
    value
}

static HOST: std::sync::atomic::AtomicPtr<Host> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
/// The host's wake, for the I/O thread: set once, at init.
static WAKE: std::sync::OnceLock<(unsafe extern "C" fn(*mut c_void), usize)> =
    std::sync::OnceLock::new();

/// Tell the host a parked task can run again. Any thread.
pub(crate) fn wake_host() {
    if let Some((wake, ctx)) = WAKE.get() {
        // SAFETY: the host's contract: `wake` may be called from any
        // thread with the context it gave.
        unsafe { wake(*ctx as *mut c_void) };
    }
}

/// The record of the call `task` belongs to.
///
/// # Safety
///
/// `task` is a task of a host call, on the worker.
unsafe fn record_of<'a>(task: *mut Task) -> &'a mut CallRecord {
    // SAFETY: the caller's contract; a short reference to the host's state.
    unsafe { &mut host().calls[(*task).wait_value] }
}

/// The task `task` failed: its error is reported once its call's last task
/// has ended, unless something awaits it first. Called on the worker, from
/// `baml_rt_task_fail`.
///
/// # Safety
///
/// `task` is a task of a host call, on the worker.
pub(crate) unsafe fn task_failed(task: *mut Task) {
    // SAFETY: the caller's contract.
    unsafe { record_of(task).failed += 1 };
}

/// The task `child` was spawned by `parent`: it belongs to its call.
/// Called from `spawn_prepare`.
///
/// # Safety
///
/// Both are tasks in use, `parent` of a host call.
pub(crate) unsafe fn inherit_call(parent: *mut Task, child: *mut Task) {
    // SAFETY: the caller's contract.
    unsafe {
        (*child).wait_value = (*parent).wait_value;
        record_of(parent).live += 1;
    }
}

/// The task `task` has ended. When it was its call's last, the call's
/// failures are reported at the end of the turn and its record is freed.
/// Called from `exit_pick`, on the task's stack.
///
/// # Safety
///
/// `task` is a task of a host call, on the worker.
pub(crate) unsafe fn task_ended(task: *mut Task) {
    // SAFETY: the caller's contract.
    unsafe {
        let number = (*task).wait_value;
        let host = host();
        let record = &mut host.calls[number];
        record.live -= 1;
        if record.live == 0 {
            if record.failed > 0 {
                host.to_report.push(number);
            } else {
                host.free_calls.push(number);
            }
        }
    }
}

/// Whether `baml_lib_init` has run. Every function but those that read
/// only the table of exports checks it first and refuses before it has.
fn initialized() -> bool {
    !HOST.load(std::sync::atomic::Ordering::Acquire).is_null()
}

/// The host's state. A raw pointer: a turn runs compiled code that calls
/// back into this module (`baml_rt_host_start`, `baml_rt_host_finish`), and
/// `on_done` may call `baml_lib_call_start`, so no reference to it is held
/// across a call out. Each function makes a short one with [`host`].
fn host_ptr() -> *mut Host {
    let host = HOST.load(std::sync::atomic::Ordering::Acquire);
    assert!(!host.is_null(), "baml_lib_init has not run");
    host
}

/// A reference to the host's state, for a function that calls out to
/// nothing that comes back into this module while it holds it.
///
/// # Safety
///
/// `baml_lib_init` has run, no other call of this module is running, and
/// the reference is dropped before anything is called that can come back.
unsafe fn host<'a>() -> &'a mut Host {
    // SAFETY: the caller's contract.
    unsafe { &mut *host_ptr() }
}

/// A slice of `length` values or bytes at `data`, which may be null when
/// `length` is zero.
///
/// # Safety
///
/// Unless `length` is zero, `data` is `length` readable items.
unsafe fn items<'a, T>(data: *const T, length: usize) -> &'a [T] {
    if length == 0 {
        &[]
    } else {
        // SAFETY: the caller's contract.
        unsafe { std::slice::from_raw_parts(data, length) }
    }
}

/// Copy `bytes` into `out`, at most `capacity` of them. `out` may be null
/// when `capacity` is zero: a call that only asks for the length.
///
/// # Safety
///
/// Unless `capacity` is zero, `out` is `capacity` writable bytes.
unsafe fn copy_out(bytes: &[u8], out: *mut u8, capacity: usize) {
    let count = bytes.len().min(capacity);
    if count > 0 {
        // SAFETY: the caller's contract.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, count) };
    }
}

fn exports() -> usize {
    &raw const baml_native_exports as usize
}

fn export_count() -> usize {
    // SAFETY: the table's header.
    unsafe { ((exports() + 4) as *const u32).read() as usize }
}

fn record(index: usize) -> usize {
    types::function_record(exports(), index)
}

/// The text at the `(offset, length)` pair of `u32`s at `at`.
fn text_at(at: usize) -> &'static str {
    // SAFETY: the compiler writes UTF-8 names inside the table.
    unsafe {
        let offset = (at as *const u32).read() as usize;
        let length = ((at + 4) as *const u32).read() as usize;
        std::str::from_utf8_unchecked(std::slice::from_raw_parts(
            (exports() + offset) as *const u8,
            length,
        ))
    }
}

/// The version of this interface ([`ABI_VERSION`]).
#[unsafe(no_mangle)]
pub extern "C" fn baml_lib_abi_version() -> u32 {
    ABI_VERSION
}

/// Build the worker. Returns zero; -1 when it was built already; -2 when
/// `callbacks` is null or older than this library's [`Callbacks`]; -3 when
/// the library's table of exports is of another version than this
/// runtime reads.
///
/// # Safety
///
/// Called before anything else here, with callbacks that keep the promises
/// of the module's documentation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_init(callbacks: *const Callbacks) -> i32 {
    if !HOST.load(std::sync::atomic::Ordering::Acquire).is_null() {
        return -1;
    }
    // SAFETY: the caller's contract: null or a struct of at least the size
    // it says.
    let callbacks = unsafe {
        if callbacks.is_null() || ((*callbacks).size as usize) < size_of::<Callbacks>() {
            return -2;
        }
        *callbacks
    };
    let stress = std::env::var_os(crate::GC_STRESS_VARIABLE).is_some();
    crate::map::randomize_seed();
    // SAFETY: the library defines the tables in the compiler's formats.
    let mut worker = Box::new(unsafe {
        Mutator::new(
            Program {
                stack_maps: &raw const baml_native_stack_maps as usize,
                types: &raw const baml_native_types as usize,
                unwind: &raw const baml_native_unwind as usize,
                functions: &raw const baml_native_functions as usize,
            },
            stress,
        )
    });
    // SAFETY: just checked at init below, before anything reads it.
    let version = unsafe { (exports() as *const u32).read() };
    if version != types::EXPORTS_VERSION {
        return -3;
    }
    table::init();
    // SAFETY: the library's own tables.
    let interface =
        unsafe { types::Interface::read(exports(), &raw const baml_native_types as usize) };
    worker.sched.task_shim = baml_native_task_shim as *const () as usize;
    let worker = Box::into_raw(worker);
    WORKER.store(worker, std::sync::atomic::Ordering::Release);
    let _ = WAKE.set((callbacks.wake, callbacks.ctx as usize));
    let host = Box::new(Host {
        worker,
        interface,
        ended: Vec::new(),
        reporting: Vec::new(),
        next_call: 1,
        callbacks,
        calls: Vec::new(),
        free_calls: Vec::new(),
        to_report: Vec::new(),
        error: String::new(),
    });
    HOST.store(Box::into_raw(host), std::sync::atomic::Ordering::Release);
    0
}

/// The number of the exported function named `name`, or -1.
///
/// # Safety
///
/// `name` is `length` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_find(name: *const u8, length: usize) -> i64 {
    // SAFETY: the caller's contract.
    let name = unsafe { items(name, length) };
    (0..export_count())
        .find(|index| text_at(record(*index) + types::FUNCTION_NAME).as_bytes() == name)
        .map_or(-1, |index| i64::try_from(index).unwrap_or(-1))
}

/// How many functions the library exports.
#[unsafe(no_mangle)]
pub extern "C" fn baml_lib_export_count() -> u64 {
    export_count() as u64
}

/// Of export `index`: its arity, the hash of its signature, and its
/// signature as text (`add(a: int, b: int) -> int`), copied into `text`
/// up to `capacity` bytes. Returns the text's full length, or -1.
///
/// # Safety
///
/// The out pointers are writable, `text` for `capacity` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_export_info(
    index: u64,
    arity: *mut u32,
    hash: *mut u64,
    text: *mut u8,
    capacity: usize,
) -> i64 {
    let Ok(index) = usize::try_from(index) else {
        return -1;
    };
    if index >= export_count() {
        return -1;
    }
    let at = record(index);
    let signature = text_at(at + types::FUNCTION_SIGNATURE);
    // SAFETY: the table's record, and the caller's contract.
    unsafe {
        *arity = ((at + types::FUNCTION_ARITY) as *const u32).read();
        *hash = ((at + types::FUNCTION_HASH) as *const u64).read();
        copy_out(signature.as_bytes(), text, capacity);
    }
    i64::try_from(signature.len()).unwrap_or(-1)
}

/// The library's interface as JSON, as `native_pack --lib --describe`
/// prints it: every export with its parameters, and every class and enum
/// with its fields and variants, numbered. Copied into `out`, at most
/// `capacity` bytes. Returns the text's full length. Reads only the table
/// of exports: it may be called before `baml_lib_init`.
///
/// # Safety
///
/// `out` is `capacity` writable bytes (null when `capacity` is zero).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_interface_json(out: *mut u8, capacity: usize) -> i64 {
    let json = text_at(exports() + 24);
    // SAFETY: the caller's contract.
    unsafe { copy_out(json.as_bytes(), out, capacity) };
    i64::try_from(json.len()).unwrap_or(-1)
}

/// The hash of the whole exported interface: what generated bindings
/// compare with their own before calling anything.
#[unsafe(no_mangle)]
pub extern "C" fn baml_lib_interface_hash() -> u64 {
    // SAFETY: the table's header.
    unsafe { ((exports() + 8) as *const u64).read() }
}

/// Start export `index` with the `count` values at `args`, each a scalar
/// or an object's handle (its tag with [`HANDLE_BIT`]), checked against
/// the parameters' types; `OMITTED` for a parameter with a default.
/// Returns the call's id, never zero, or zero when the export or an
/// argument is refused (see `baml_lib_last_error`). The call runs in the
/// next turn. The handles passed stay the host's, but for an array or a
/// map, which is taken: it can be passed once.
///
/// # Safety
///
/// `args` is `count` readable values, or null when `count` is zero. Not
/// during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_call_start(index: u64, args: *const Value, count: usize) -> u64 {
    if !initialized() {
        return 0;
    }
    let Some(index) = usize::try_from(index)
        .ok()
        .filter(|index| *index < export_count())
    else {
        return refuse(format!("no export {index}"), 0);
    };
    // SAFETY: the caller's contract.
    let passed = unsafe { items(args, count) };
    if let Err(why) = values::check_arguments(index, passed) {
        return refuse(why, 0);
    }
    // SAFETY: the caller's contract; no call out follows.
    let host = unsafe { host() };
    let entry = host.interface.functions[index].entry;
    host.worker().clear_anchor();
    // The entry and the arguments wait in an array, which the task holds as
    // what it is to run: a root until the task takes it. Making it may
    // collect, which the handles survive, so they are read after.
    // SAFETY: a worker with no task running.
    let array = unsafe { crate::array::array_new(host.worker(), 1 + count) };
    // SAFETY: an array of `1 + count` elements has storage for them, and
    // nothing allocates from here on.
    unsafe {
        let elements = load(array + ARRAY_PTR) + STORAGE_ELEMENTS;
        // The entry's address as an `int`, which the collector leaves alone.
        Value::init(
            elements,
            Value {
                tag: TAG_INT,
                payload: entry,
            },
        );
        for (place, value) in passed.iter().enumerate() {
            let value = values::resolve(*value, true).expect("checked above");
            Value::init(elements + (1 + place) * VALUE, value);
        }
    }
    // SAFETY: the library defines its shim.
    let shim = baml_native_host_shim as *const () as usize;
    let task = host.worker().sched.new_task(TASK_STACK, shim);
    let id = host.next_call;
    host.next_call += 1;
    let record = CallRecord {
        id,
        live: 1,
        ..CallRecord::default()
    };
    let number = match host.free_calls.pop() {
        Some(number) => {
            host.calls[number] = record;
            number
        }
        None => {
            host.calls.push(record);
            host.calls.len() - 1
        }
    };
    // SAFETY: a new task, this function's until it is queued.
    unsafe {
        (*task).entry = array;
        (*task).wait_value = number;
    }
    // Calls run in the order they were started: the queue is served from
    // its back.
    host.worker().sched.queue.push_front(task);
    id
}

/// What the host shim of a task the host started asks for first: the entry
/// to call, and the address of the arguments, `2 * arity` words, which the
/// entry reads before anything can collect.
#[unsafe(no_mangle)]
pub extern "C" fn baml_rt_host_start(worker: &mut Mutator) -> Value {
    worker.sched.settle_last_switch();
    let task = worker.cur_task;
    // SAFETY: called on a task the host started, whose array of the entry
    // and arguments is a live array of at least one element.
    unsafe {
        let array = std::mem::take(&mut (*task).entry);
        let elements = load(array + ARRAY_PTR) + STORAGE_ELEMENTS;
        Value {
            tag: Value::unshared(elements).payload,
            payload: elements + VALUE,
        }
    }
}

/// The entry of a task the host started returned the value `tag`,
/// `payload`: what was thrown when `tag` has `THROWN`. Kept for the end of
/// the turn; the task ends when this returns. Allocates nothing.
#[unsafe(no_mangle)]
pub extern "C" fn baml_rt_host_finish(worker: &mut Mutator, tag: usize, payload: usize) {
    let task = worker.cur_task;
    // SAFETY: called on a task the host started.
    let record = unsafe { record_of(task) };
    record.root_done = true;
    if record.abandoned {
        // Reported stuck: the host has been told the call is over.
        return;
    }
    let call = record.id;
    let value = Value {
        tag: tag & !THROWN,
        payload,
    };
    let status = if tag & THROWN == 0 {
        STATUS_RETURNED
    // SAFETY: an instance's header is its first word.
    } else if value.tag == TAG_INSTANCE && unsafe { load(value.payload) } & HEADER_PANIC != 0 {
        STATUS_PANICKED
    } else {
        STATUS_THREW
    };
    let value = values::crossing(value);
    // SAFETY: called on a task the host started; no call out follows.
    let host = unsafe { host() };
    host.ended.push(Ended {
        call,
        status,
        value,
    });
}

/// Run every task that can run, until each has ended or parked, then
/// report the calls that ended and the failures nobody awaits. Never
/// sleeps. Returns -1 when no task waits for a timer, and otherwise the
/// nanoseconds until the next fires, when the host is to turn again.
/// Parked I/O is reported through `wake`.
///
/// # Safety
///
/// Not during a turn, nor with another call of this module running.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_turn() -> i64 {
    if !initialized() {
        return -1;
    }
    let host = host_ptr();
    // SAFETY: the caller's contract. Only short references to the host's
    // state are made, none held across the run or a callback.
    let (worker, callbacks) = unsafe { ((*host).worker, (*host).callbacks) };
    let mut unhandled = Vec::new();
    loop {
        // SAFETY: on the host's stack, which serves as the worker's.
        unsafe { run_until_parked(worker) };
        // SAFETY: as above; nothing here runs BAML or calls out.
        unsafe {
            if stuck(&(*worker).sched) {
                abandon_stuck(&mut *host);
            }
            if !(*host).to_report.is_empty() {
                take_unhandled(&mut *host, &mut unhandled);
            }
        }
        // SAFETY: as above.
        let mut ended = unsafe {
            let host = &mut *host;
            std::mem::swap(&mut host.ended, &mut host.reporting);
            std::mem::take(&mut host.reporting)
        };
        for Ended {
            call,
            status,
            value,
        } in ended.drain(..)
        {
            // SAFETY: the host's contract. Back on the host's own stack.
            unsafe {
                (callbacks.on_done)(
                    callbacks.ctx,
                    call,
                    status,
                    value.tag as u64,
                    value.payload as u64,
                );
            }
        }
        // SAFETY: as above: the emptied vector is kept for the next turn.
        unsafe { (*host).reporting = ended };
        for (call, value) in unhandled.drain(..) {
            // SAFETY: as above.
            unsafe {
                (callbacks.on_unhandled)(
                    callbacks.ctx,
                    call,
                    value.tag as u64,
                    value.payload as u64,
                );
            }
        }
        // `on_done` may have started more calls.
        // SAFETY: as above.
        if unsafe { (*worker).sched.queue.is_empty() } {
            break;
        }
    }
    // SAFETY: as above.
    let sched = unsafe { &(*worker).sched };
    sched
        .timers
        .peek()
        .map_or(-1, |std::cmp::Reverse((when, ..))| {
            i64::try_from(when.saturating_sub(sched.now())).unwrap_or(i64::MAX)
        })
}

/// Whether nothing the worker has can ever run again: no task is
/// runnable, no timer is set, no woken task waits in the inbox, and every
/// task parked on I/O waits for something only another task can end, as
/// the executable's worker decides before it reports a deadlock.
fn stuck(sched: &super::Sched) -> bool {
    sched.queue.is_empty()
        && sched.timers.is_empty()
        && !sched
            .inbox
            .filled
            .load(std::sync::atomic::Ordering::Acquire)
        && sched.on_io == sched.held_up.len()
        && sched.held_up.iter().all(|(_, held_up)| held_up())
}

/// Report every call still in flight as stuck, and its failures with it.
/// Its tasks stay parked: one parked on I/O may still be named by the job
/// it waits for, so it cannot be freed.
fn abandon_stuck(host: &mut Host) {
    for (number, record) in host.calls.iter_mut().enumerate() {
        if record.live > 0 && !record.root_done && !record.abandoned {
            record.abandoned = true;
            host.ended.push(Ended {
                call: record.id,
                status: STATUS_STUCK,
                value: Value::NULL,
            });
            if record.failed > 0 {
                host.to_report.push(number);
            }
        }
    }
}

/// Take the error of every failed task of the calls to report, as a value
/// that crosses, and free the task. Nothing can await it any more: its
/// call's last task has ended, or the call is stuck. Then free the records
/// of the calls that have no task left.
fn take_unhandled(host: &mut Host, unhandled: &mut Vec<(u64, Value)>) {
    let reported = std::mem::take(&mut host.to_report);
    // SAFETY: made at init and never freed.
    let sched = unsafe { &mut (*host.worker).sched };
    let failed: Vec<*mut Task> = sched
        .tasks()
        .filter(|&task| {
            // SAFETY: a task in use.
            unsafe {
                (*task).flags & TASK_FAILED != 0
                    && (*task).state.load(std::sync::atomic::Ordering::Acquire) == DEAD
                    && reported.contains(&(*task).wait_value)
            }
        })
        .collect();
    for task in failed {
        // SAFETY: a failed task that has ended, whose future holds its
        // error, and which is not the task the worker left last (a turn
        // has settled that one).
        unsafe {
            let future = (*task).future;
            let error = Value::unshared(future + FUTURE_VALUE);
            store(
                future + FUTURE_FLAGS,
                load(future + FUTURE_FLAGS) | FUTURE_OBSERVED,
            );
            store(future + FUTURE_TASK, 0);
            let record = &mut host.calls[(*task).wait_value];
            record.failed -= 1;
            let call = record.id;
            sched.release(task);
            unhandled.push((call, values::crossing(error)));
        }
    }
    for number in reported {
        let record = &mut host.calls[number];
        if record.live == 0 {
            *record = CallRecord::default();
            host.free_calls.push(number);
        }
    }
}

/// The loop of `serve`, which returns where `serve` would sleep.
///
/// # Safety
///
/// Called on the host's stack, with no task running.
unsafe fn run_until_parked(worker: *mut Mutator) {
    // SAFETY: the caller's contract; as `serve`.
    unsafe {
        loop {
            let sched = &mut (*worker).sched;
            sched.settle_last_switch();
            sched.take_woken();
            sched.fire_timers();
            let Some(task) = sched.queue.pop_back() else {
                return;
            };
            (*task)
                .state
                .store(RUNNING, std::sync::atomic::Ordering::Release);
            baml_rt_resume(worker, task);
            let left = std::mem::replace(&mut (*worker).cur_task, std::ptr::null_mut());
            if (*left).state.compare_exchange(
                PARKING,
                PARKED,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            ) == Err(NOTIFIED)
            {
                (*left)
                    .state
                    .store(RUNNABLE, std::sync::atomic::Ordering::Release);
                (*worker).sched.queue.push_back(left);
            }
        }
    }
}

/// How many tasks are in use: for a test that tasks are freed.
///
/// # Safety
///
/// Not during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_debug_task_count() -> u64 {
    if !initialized() {
        return 0;
    }
    // SAFETY: the caller's contract.
    let host = unsafe { host() };
    host.worker().sched.tasks().count() as u64
}

/// A handle to a new string holding the UTF-8 text at `text`, or zero
/// when it is not UTF-8. May collect.
///
/// # Safety
///
/// `text` is `length` readable bytes (null when `length` is zero). Not
/// during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_string_new(text: *const u8, length: usize) -> u64 {
    if !initialized() {
        return 0;
    }
    // SAFETY: the caller's contract.
    let bytes = unsafe { items(text, length) };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return refuse("a string that is not UTF-8".to_owned(), 0);
    };
    // SAFETY: as above; no reference to the host's state is held across
    // the allocation.
    let string = unsafe {
        let worker = host().worker();
        worker.clear_anchor();
        crate::string::from_str(worker, text)
    };
    table::hold(
        Value {
            tag: TAG_STRING,
            payload: string,
        },
        0,
    ) as u64
}

/// The tag of the object `handle` holds (without [`HANDLE_BIT`]), or
/// `u64::MAX` for a handle that is not live.
///
/// # Safety
///
/// Not during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_handle_tag(handle: u64) -> u64 {
    if !initialized() {
        return u64::MAX;
    }
    table::get(handle).map_or(u64::MAX, |value| value.tag as u64)
}

/// The string `handle` holds, if it holds one.
fn string_of(handle: u64) -> Result<&'static str, String> {
    let value = table::get(handle).ok_or_else(|| format!("handle {handle} is not live"))?;
    if !(TAG_STRING..STRING_TAGS_END).contains(&value.tag) {
        return Err(format!("handle {handle} holds no string"));
    }
    // SAFETY: a live string, read before anything can move it.
    Ok(unsafe { crate::string::text(value.payload) })
}

/// The bytes of UTF-8 text of the string `handle` holds, or -1.
///
/// # Safety
///
/// Not during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_string_len(handle: u64) -> i64 {
    if !initialized() {
        return -1;
    }
    match string_of(handle) {
        Ok(text) => i64::try_from(text.len()).unwrap_or(-1),
        Err(why) => refuse(why, -1),
    }
}

/// Copy the text of the string `handle` holds into `out`, at most
/// `capacity` bytes. Returns the text's full length, or -1. Allocates
/// nothing.
///
/// # Safety
///
/// `out` is `capacity` writable bytes (null when `capacity` is zero). Not
/// during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_string_copy(handle: u64, out: *mut u8, capacity: usize) -> i64 {
    if !initialized() {
        return -1;
    }
    match string_of(handle) {
        Ok(text) => {
            // SAFETY: the caller's contract.
            unsafe { copy_out(text.as_bytes(), out, capacity) };
            i64::try_from(text.len()).unwrap_or(-1)
        }
        Err(why) => refuse(why, -1),
    }
}

/// Write the value `handle` holds as an uncaught throw of it is written
/// (an error's class and fields, say) into `out`, at most `capacity` bytes.
/// Returns the text's full length, or -1. Allocates nothing on the heap.
///
/// # Safety
///
/// `out` is `capacity` writable bytes (null when `capacity` is zero). Not
/// during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_describe(handle: u64, out: *mut u8, capacity: usize) -> i64 {
    if !initialized() {
        return -1;
    }
    let Some(value) = table::get(handle) else {
        return refuse(format!("handle {handle} is not live"), -1);
    };
    let mut text = String::new();
    // SAFETY: a live value; the caller's contract.
    unsafe {
        crate::class::write_value(host().worker(), &mut text, value, None);
        copy_out(text.as_bytes(), out, capacity);
    }
    i64::try_from(text.len()).unwrap_or(-1)
}

/// Why the last function that refused something refused it, copied into
/// `out`, at most `capacity` bytes. Returns the text's full length.
///
/// # Safety
///
/// `out` is `capacity` writable bytes (null when `capacity` is zero). Not
/// during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_last_error(out: *mut u8, capacity: usize) -> i64 {
    if !initialized() {
        return -1;
    }
    // SAFETY: the caller's contract.
    unsafe {
        let error = &host().error;
        copy_out(error.as_bytes(), out, capacity);
        i64::try_from(error.len()).unwrap_or(-1)
    }
}

/// Let go of `handle`. A handle that is not live is ignored.
///
/// # Safety
///
/// Not during a turn. `handle` is not used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_handle_free(handle: u64) {
    if !initialized() {
        return;
    }
    table::free(handle);
}

/// Collect now: every object moves. For tests of handles.
///
/// # Safety
///
/// Not during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_collect() {
    if !initialized() {
        return;
    }
    // SAFETY: the caller's contract.
    let worker = unsafe { host().worker() };
    worker.clear_anchor();
    worker.collect(0);
}

/// The address of the object `handle` holds now: for a test that a
/// collection moved it. Never to be dereferenced by a host.
///
/// # Safety
///
/// Not during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_debug_address(handle: u64) -> u64 {
    if !initialized() {
        return 0;
    }
    table::get(handle).map_or(0, |value| value.payload as u64)
}

/// How many handles are live: for a test that a host frees them all.
///
/// # Safety
///
/// Not during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_debug_handle_count() -> u64 {
    if !initialized() {
        return 0;
    }
    table::live() as u64
}
