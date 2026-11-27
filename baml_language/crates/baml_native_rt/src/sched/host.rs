//! The library form: a compiled program a host program loads and calls.
//!
//! A spike: see the report of the native bridges spike. The program is linked as a shared library with
//! no `main` and no thread of its own. The host starts calls and gives the
//! worker turns, on its own thread:
//!
//! ```text
//! baml_lib_init(on_done, wake, ctx)      build the worker
//! baml_lib_call_start(fn, args, n)       a task for the call, queued; a call id
//! baml_lib_turn()                        run tasks until each is done or parked
//!    -> on_done(ctx, call, tag, payload) for each call that ended, after the run
//!    -> wake(ctx)                        from the I/O thread: give me a turn
//! ```
//!
//! A turn never sleeps: where the executable's worker waits on its inbox,
//! a turn returns, and the I/O thread that fills the inbox also calls the
//! host's `wake`. A task that sleeps on a timer is the worker's own: a turn
//! returns how long until the next one fires, and the host turns again
//! then.
//!
//! # Values
//!
//! A value crosses as its two words. A scalar is what a slot holds. An
//! object keeps its own tag, with [`HANDLE_BIT`] set: its payload is then a
//! handle, the number of a slot of the handle table, which the collector
//! keeps current, never an address. A host holds handles across turns and
//! collections; it reads objects only through the accessors here, which
//! copy. A call that threw reports its error with `THROWN` in the tag.
//!
//! # What must hold
//!
//! One worker per process, and one thread in this module at a time: no
//! call here may run during a turn, nor two at once. Only `wake` comes from
//! another thread. The turn runs on the host's stack, which is the worker's
//! own stack for its duration.

use std::ffi::c_void;

use super::{
    NOTIFIED, PARKED, PARKING, RUNNABLE, RUNNING, TASK_STACK, WORKER, baml_rt_resume, load,
};
use crate::{
    heap::{Mutator, Program, Value},
    layout::{
        ARRAY_PTR, STORAGE_ELEMENTS, STRING_TAGS_END, TAG_STRING, THROWN, TYPE_ID_MASK, VALUE,
        is_pointer,
    },
};

/// Set in the tag of a value that crosses to or from the host when its
/// payload is a handle. Bit 61 is free in every tag and header
/// (`layout.rs`), and a value with it is never stored on the heap.
pub const HANDLE_BIT: usize = 1 << 61;

/// How many handles a host can hold at once.
const HANDLES: usize = 4096;

/// Called after a turn for each call that ended: the call id, and the
/// result or the error (`THROWN` in the tag) as a value that crosses.
pub type OnDone = unsafe extern "C" fn(ctx: *mut c_void, call: u64, tag: u64, payload: u64);
/// Called from any thread when a parked task can run again.
pub type WakeFn = unsafe extern "C" fn(ctx: *mut c_void);

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

/// The table of exports the compiler writes (`codegen/host.rs`): a version,
/// a count, the interface hash, then a record per export.
const EXPORTS_VERSION: u32 = 1;
const EXPORTS_RECORDS: usize = 16;
const EXPORT_RECORD: usize = 40;
const EXPORT_ENTRY: usize = 0;
const EXPORT_HASH: usize = 8;
const EXPORT_ARITY: usize = 16;
const EXPORT_NAME: usize = 20;
const EXPORT_SIGNATURE: usize = 28;

struct Call {
    id: u64,
    /// The compiled entry the task calls with the arguments.
    entry: usize,
}

struct Ended {
    call: u64,
    value: Value,
    threw: bool,
}

struct Host {
    worker: Box<Mutator>,
    /// The handle table: every slot is a root of the worker. Never moved.
    handles: Box<[Value]>,
    free: Vec<u32>,
    /// Each task started by the host, by its address, with its call.
    calls: std::collections::HashMap<usize, Call>,
    /// Calls that ended in this turn, reported after it.
    ended: Vec<Ended>,
    next_call: u64,
    on_done: OnDone,
    ctx: usize,
}

static HOST: std::sync::atomic::AtomicPtr<Host> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
/// The host's wake, for the I/O thread: set once, at init.
static WAKE: std::sync::OnceLock<(WakeFn, usize)> = std::sync::OnceLock::new();

/// Tell the host a parked task can run again. Any thread.
pub(crate) fn wake_host() {
    if let Some((wake, ctx)) = WAKE.get() {
        // SAFETY: the host's contract: `wake` may be called from any
        // thread with the context it gave.
        unsafe { wake(*ctx as *mut c_void) };
    }
}

/// # Safety
///
/// `baml_lib_init` has run, and no other call of this module is running.
unsafe fn host<'a>() -> &'a mut Host {
    let host = HOST.load(std::sync::atomic::Ordering::Acquire);
    assert!(!host.is_null(), "baml_lib_init has not run");
    // SAFETY: the caller's contract.
    unsafe { &mut *host }
}

fn exports() -> usize {
    &raw const baml_native_exports as usize
}

fn export_count() -> usize {
    // SAFETY: the table's header.
    unsafe { ((exports() + 4) as *const u32).read() as usize }
}

fn record(index: usize) -> usize {
    exports() + EXPORTS_RECORDS + index * EXPORT_RECORD
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

impl Host {
    /// A handle to `value`, which is a pointer.
    fn hold(&mut self, value: Value) -> usize {
        let Some(slot) = self.free.pop() else {
            super::fatal("the host holds every handle: free some");
        };
        self.handles[slot as usize] = value;
        slot as usize
    }

    /// What the host sees of `value`: itself when it is a scalar, its tag
    /// with [`HANDLE_BIT`] and a handle when it is an object.
    fn crossing(&mut self, value: Value) -> Value {
        if is_pointer(value.tag) {
            Value {
                tag: value.tag | HANDLE_BIT,
                payload: self.hold(value),
            }
        } else {
            value
        }
    }

    /// The value the host passed: a handle's object, or the scalar itself.
    fn resolve(&self, value: Value) -> Option<Value> {
        if value.tag & HANDLE_BIT == 0 {
            return (!is_pointer(value.tag)).then_some(value);
        }
        let held = *self.handles.get(value.payload)?;
        (held.tag == value.tag & !HANDLE_BIT).then_some(held)
    }

    fn held(&self, handle: u64) -> Option<Value> {
        let value = *self.handles.get(usize::try_from(handle).ok()?)?;
        is_pointer(value.tag).then_some(value)
    }
}

/// Build the worker. Returns zero, or -1 when it was built already.
///
/// # Safety
///
/// Called once, before anything else here. `on_done` is called on the
/// thread that runs turns, `wake` from any thread, both with `ctx`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_init(on_done: OnDone, wake: WakeFn, ctx: *mut c_void) -> i32 {
    if !HOST.load(std::sync::atomic::Ordering::Acquire).is_null() {
        return -1;
    }
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
    let mut handles = vec![Value::NULL; HANDLES].into_boxed_slice();
    for slot in &mut handles {
        worker.root_value(slot);
    }
    worker.sched.task_shim = baml_native_task_shim as *const () as usize;
    WORKER.store(&raw mut *worker, std::sync::atomic::Ordering::Release);
    let _ = WAKE.set((wake, ctx as usize));
    let host = Box::new(Host {
        worker,
        handles,
        free: (0..u32::try_from(HANDLES).expect("few handles"))
            .rev()
            .collect(),
        calls: std::collections::HashMap::new(),
        ended: Vec::new(),
        next_call: 1,
        on_done,
        ctx: ctx as usize,
    });
    HOST.store(Box::into_raw(host), std::sync::atomic::Ordering::Release);
    // SAFETY: just built.
    let version = unsafe { (exports() as *const u32).read() };
    if version != EXPORTS_VERSION {
        super::fatal("the library's table of exports is of another version");
    }
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
    let name = unsafe { std::slice::from_raw_parts(name, length) };
    (0..export_count())
        .find(|index| text_at(record(*index) + EXPORT_NAME).as_bytes() == name)
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
    let signature = text_at(at + EXPORT_SIGNATURE);
    // SAFETY: the table's record, and the caller's contract.
    unsafe {
        *arity = ((at + EXPORT_ARITY) as *const u32).read();
        *hash = ((at + EXPORT_HASH) as *const u64).read();
        let copied = signature.len().min(capacity);
        std::ptr::copy_nonoverlapping(signature.as_ptr(), text, copied);
    }
    i64::try_from(signature.len()).unwrap_or(-1)
}

/// The hash of the whole exported interface: what generated bindings
/// compare with their own before calling anything.
#[unsafe(no_mangle)]
pub extern "C" fn baml_lib_interface_hash() -> u64 {
    // SAFETY: the table's header.
    unsafe { ((exports() + 8) as *const u64).read() }
}

/// Start export `index` with the `count` values at `args`, each a scalar
/// or an object's handle (its tag with [`HANDLE_BIT`]). Returns the call's
/// id, never zero, or zero when the export or the arguments are wrong. The
/// call runs in the next turn. The handles passed stay the host's.
///
/// # Safety
///
/// `args` is `count` readable values. Not during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_call_start(index: u64, args: *const Value, count: usize) -> u64 {
    // SAFETY: the caller's contract.
    let host = unsafe { host() };
    let Ok(index) = usize::try_from(index) else {
        return 0;
    };
    if index >= export_count() {
        return 0;
    }
    let at = record(index);
    // SAFETY: the table's record.
    let (entry, arity) = unsafe {
        (
            load(at + EXPORT_ENTRY),
            ((at + EXPORT_ARITY) as *const u32).read() as usize,
        )
    };
    if count != arity {
        return 0;
    }
    // SAFETY: the caller's contract.
    let passed = unsafe { std::slice::from_raw_parts(args, count) };
    if passed.iter().any(|value| host.resolve(*value).is_none()) {
        return 0;
    }
    host.worker.clear_anchor();
    // The arguments wait in an array, which the task holds as what it is to
    // run: a root until the task takes it. Making it may collect, which the
    // handles survive, so they are read after.
    // SAFETY: a worker with no task running.
    let array = unsafe { crate::array::array_new(&mut host.worker, count) };
    if count > 0 {
        // SAFETY: an array of `count` elements has storage for them.
        unsafe {
            let elements = load(array + ARRAY_PTR) + STORAGE_ELEMENTS;
            for (place, value) in passed.iter().enumerate() {
                let value = host.resolve(*value).expect("checked above");
                Value::init(elements + place * VALUE, value);
            }
        }
    }
    // SAFETY: the library defines its shim.
    let shim = baml_native_host_shim as *const () as usize;
    let task = host.worker.sched.new_task(TASK_STACK, shim);
    let id = host.next_call;
    host.next_call += 1;
    // SAFETY: a new task, this function's until it is queued.
    unsafe {
        (*task).entry = array;
    }
    host.calls.insert(task as usize, Call { id, entry });
    // Calls run in the order they were started: the queue is served from
    // its back.
    host.worker.sched.queue.push_front(task);
    id
}

/// What the host shim of a task the host started asks for first: the entry
/// to call, and the address of the arguments, `2 * arity` words, which the
/// entry reads before anything can collect.
#[unsafe(no_mangle)]
pub extern "C" fn baml_rt_host_start(worker: &mut Mutator) -> Value {
    worker.sched.settle_last_switch();
    let task = worker.cur_task;
    // SAFETY: called on a task the host started, whose array of arguments
    // is a live array.
    unsafe {
        let host = host();
        let entry = host.calls[&(task as usize)].entry;
        let array = std::mem::take(&mut (*task).entry);
        let storage = load(array + ARRAY_PTR);
        let args = if storage == 0 {
            0
        } else {
            storage + STORAGE_ELEMENTS
        };
        Value {
            tag: entry,
            payload: args,
        }
    }
}

/// The entry of a task the host started returned the value `tag`,
/// `payload`: what was thrown when `tag` has `THROWN`. Kept for the end of
/// the turn; the task ends when this returns. Allocates nothing.
#[unsafe(no_mangle)]
pub extern "C" fn baml_rt_host_finish(worker: &mut Mutator, tag: usize, payload: usize) {
    let task = worker.cur_task as usize;
    // SAFETY: called on a task the host started.
    let host = unsafe { host() };
    let call = host.calls.remove(&task).expect("a task the host started");
    let threw = tag & THROWN != 0;
    let value = host.crossing(Value {
        tag: tag & !THROWN,
        payload,
    });
    host.ended.push(Ended {
        call: call.id,
        value,
        threw,
    });
}

/// Run every task that can run, until each has ended or parked, then
/// report the calls that ended. Never sleeps. Returns -1 when no task waits
/// for a timer, and otherwise the nanoseconds until the next fires, when
/// the host is to turn again. Parked I/O is reported through `wake`.
///
/// # Safety
///
/// Not during a turn, nor with another call of this module running.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_turn() -> i64 {
    // SAFETY: the caller's contract.
    let host = unsafe { host() };
    let worker = &raw mut *host.worker;
    loop {
        // SAFETY: on the host's stack, which serves as the worker's.
        unsafe { run_until_parked(worker) };
        let ended = std::mem::take(&mut host.ended);
        for Ended { call, value, threw } in ended {
            let tag = value.tag | if threw { THROWN } else { 0 };
            // SAFETY: the host's contract. Back on the host's own stack.
            unsafe {
                (host.on_done)(
                    host.ctx as *mut c_void,
                    call,
                    tag as u64,
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

/// A handle to a new string holding the UTF-8 text at `text`, or `u64::MAX`
/// when it is not UTF-8. May collect.
///
/// # Safety
///
/// `text` is `length` readable bytes. Not during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_string_new(text: *const u8, length: usize) -> u64 {
    // SAFETY: the caller's contract.
    let (host, bytes) = unsafe { (host(), std::slice::from_raw_parts(text, length)) };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return u64::MAX;
    };
    host.worker.clear_anchor();
    let string = crate::string::from_str(&mut host.worker, text);
    host.hold(Value {
        tag: TAG_STRING,
        payload: string,
    }) as u64
}

/// The tag of the object `handle` holds (without [`HANDLE_BIT`]), or
/// `u64::MAX` for a free or wrong handle.
///
/// # Safety
///
/// Not during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_handle_tag(handle: u64) -> u64 {
    // SAFETY: the caller's contract.
    let host = unsafe { host() };
    host.held(handle).map_or(u64::MAX, |value| value.tag as u64)
}

/// The bytes of UTF-8 text of the string `handle` holds, or -1.
///
/// # Safety
///
/// Not during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_string_len(handle: u64) -> i64 {
    // SAFETY: the caller's contract.
    let host = unsafe { host() };
    match host.held(handle) {
        Some(value) if (value.tag & TYPE_ID_MASK) < STRING_TAGS_END && value.tag >= TAG_STRING => {
            // SAFETY: a live string.
            let text = unsafe { crate::string::text(value.payload) };
            i64::try_from(text.len()).unwrap_or(-1)
        }
        _ => -1,
    }
}

/// Copy the text of the string `handle` holds into `out`, at most
/// `capacity` bytes. Returns the text's full length, or -1. Allocates
/// nothing.
///
/// # Safety
///
/// `out` is `capacity` writable bytes. Not during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_string_copy(handle: u64, out: *mut u8, capacity: usize) -> i64 {
    // SAFETY: the caller's contract.
    let host = unsafe { host() };
    match host.held(handle) {
        Some(value) if (value.tag & TYPE_ID_MASK) < STRING_TAGS_END && value.tag >= TAG_STRING => {
            // SAFETY: a live string, and the caller's contract.
            unsafe {
                let text = crate::string::text(value.payload);
                std::ptr::copy_nonoverlapping(text.as_ptr(), out, text.len().min(capacity));
                i64::try_from(text.len()).unwrap_or(-1)
            }
        }
        _ => -1,
    }
}

/// Write the value `handle` holds as an uncaught throw of it is written
/// (an error's class and fields, say) into `out`, at most `capacity` bytes.
/// Returns the text's full length, or -1. Allocates nothing on the heap.
///
/// # Safety
///
/// `out` is `capacity` writable bytes. Not during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_describe(handle: u64, out: *mut u8, capacity: usize) -> i64 {
    // SAFETY: the caller's contract.
    let host = unsafe { host() };
    let Some(value) = host.held(handle) else {
        return -1;
    };
    let mut text = String::new();
    // SAFETY: a live value.
    unsafe {
        crate::class::write_value(&host.worker, &mut text, value, None);
        std::ptr::copy_nonoverlapping(text.as_ptr(), out, text.len().min(capacity));
    }
    i64::try_from(text.len()).unwrap_or(-1)
}

/// Let go of `handle`.
///
/// # Safety
///
/// Not during a turn. `handle` is not used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_handle_free(handle: u64) {
    // SAFETY: the caller's contract.
    let host = unsafe { host() };
    if host.held(handle).is_some() {
        let slot = usize::try_from(handle).expect("checked");
        host.handles[slot] = Value::NULL;
        host.free.push(u32::try_from(slot).expect("few handles"));
    }
}

/// Collect now: every object moves. For tests of handles.
///
/// # Safety
///
/// Not during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_collect() {
    // SAFETY: the caller's contract.
    let host = unsafe { host() };
    host.worker.clear_anchor();
    host.worker.collect(0);
}

/// The address of the object `handle` holds now: for a test that a
/// collection moved it. Never to be dereferenced by a host.
///
/// # Safety
///
/// Not during a turn.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_lib_debug_address(handle: u64) -> u64 {
    // SAFETY: the caller's contract.
    let host = unsafe { host() };
    host.held(handle).map_or(0, |value| value.payload as u64)
}
