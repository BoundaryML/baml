//! A host program for the native bridges spike: it loads a BAML program
//! compiled as a shared library (`native_pack --lib`) and awaits its
//! functions as Rust futures, on a minimal executor of its own. It needs
//! nothing of the compiler: only the library's `baml_lib_*` functions,
//! found with `dlopen`.
//!
//! ```text
//! native_pack --lib examples/native_host/bridge.baml libbridge.dylib add nap env_or greet fail
//! native_host libbridge.dylib
//! native_host libbridge.dylib --bench <n> serial|batch
//! ```
//!
//! `--bench` measures the floor of a call: `add(i, 1)` `n` times, each
//! started and given its own turn (`serial`), or all started and then run
//! in one turn (`batch`), with an `on_done` that only sums the results.
//!
//! The model: BAML runs on this thread, in turns. A call starts at once and
//! completes in a later turn, reported through `on_done`. A task that parks
//! on I/O is woken by the library's I/O thread, which calls `wake`; one
//! that sleeps on a timer is reported by `baml_lib_turn`'s result.

#![allow(
    unsafe_code,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::missing_transmute_annotations,
    reason = "a host program for a C interface, which prints what it saw and finds its \
              functions by name"
)]

use std::{
    collections::HashMap,
    ffi::{CString, c_char, c_int, c_void},
    future::Future,
    pin::Pin,
    sync::{Arc, Condvar, Mutex},
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};

/// A value as it crosses: a tag and a payload.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug)]
struct Value {
    tag: u64,
    payload: u64,
}

const TAG_TYPE_ID: u64 = 0xFFFF_FFFF;
const TAG_INT: u64 = 2;
const TAG_STRING: u64 = 5;
const THROWN: u64 = 1 << 59;
const HANDLE: u64 = 1 << 61;

type OnDone = unsafe extern "C" fn(*mut c_void, u64, u64, u64);
type WakeFn = unsafe extern "C" fn(*mut c_void);

unsafe extern "C" {
    fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlerror() -> *const c_char;
    fn pthread_self() -> usize;
    fn pthread_getname_np(thread: usize, name: *mut c_char, length: usize) -> c_int;
}

const RTLD_NOW: c_int = 2;

/// The library's functions.
struct Lib {
    init: unsafe extern "C" fn(OnDone, WakeFn, *mut c_void) -> i32,
    find: unsafe extern "C" fn(*const u8, usize) -> i64,
    export_count: unsafe extern "C" fn() -> u64,
    export_info: unsafe extern "C" fn(u64, *mut u32, *mut u64, *mut u8, usize) -> i64,
    interface_hash: unsafe extern "C" fn() -> u64,
    call_start: unsafe extern "C" fn(u64, *const Value, usize) -> u64,
    turn: unsafe extern "C" fn() -> i64,
    string_new: unsafe extern "C" fn(*const u8, usize) -> u64,
    string_copy: unsafe extern "C" fn(u64, *mut u8, usize) -> i64,
    describe: unsafe extern "C" fn(u64, *mut u8, usize) -> i64,
    handle_free: unsafe extern "C" fn(u64),
    collect: unsafe extern "C" fn(),
    debug_address: unsafe extern "C" fn(u64) -> u64,
}

impl Lib {
    fn open(path: &str) -> Result<Self, String> {
        let path = CString::new(path).map_err(|error| error.to_string())?;
        // SAFETY: a path, and symbols of the types the library defines.
        unsafe {
            let handle = dlopen(path.as_ptr(), RTLD_NOW);
            if handle.is_null() {
                return Err(std::ffi::CStr::from_ptr(dlerror())
                    .to_string_lossy()
                    .into_owned());
            }
            let symbol = |name: &str| -> Result<*mut c_void, String> {
                let name = CString::new(name).expect("a symbol name");
                let address = dlsym(handle, name.as_ptr());
                if address.is_null() {
                    Err(format!("no symbol {name:?}"))
                } else {
                    Ok(address)
                }
            };
            macro_rules! function {
                ($name:literal) => {
                    std::mem::transmute(symbol($name)?)
                };
            }
            Ok(Self {
                init: function!("baml_lib_init"),
                find: function!("baml_lib_find"),
                export_count: function!("baml_lib_export_count"),
                export_info: function!("baml_lib_export_info"),
                interface_hash: function!("baml_lib_interface_hash"),
                call_start: function!("baml_lib_call_start"),
                turn: function!("baml_lib_turn"),
                string_new: function!("baml_lib_string_new"),
                string_copy: function!("baml_lib_string_copy"),
                describe: function!("baml_lib_describe"),
                handle_free: function!("baml_lib_handle_free"),
                collect: function!("baml_lib_collect"),
                debug_address: function!("baml_lib_debug_address"),
            })
        }
    }
}

/// The text a library accessor (`string_copy`, `describe`) copies out of
/// the object `handle` holds.
fn text(read: unsafe extern "C" fn(u64, *mut u8, usize) -> i64, handle: u64) -> String {
    let mut buffer = vec![0_u8; 256];
    // SAFETY: a buffer of its length.
    let length = unsafe { read(handle, buffer.as_mut_ptr(), buffer.len()) };
    let length = usize::try_from(length).expect("a live handle");
    if length > buffer.len() {
        buffer.resize(length, 0);
        // SAFETY: as above.
        unsafe { read(handle, buffer.as_mut_ptr(), buffer.len()) };
    }
    buffer.truncate(length);
    String::from_utf8(buffer).expect("UTF-8")
}

/// What the executor and the library's callbacks share.
#[derive(Default)]
struct Shared {
    /// Each call that ended, by id, with its value.
    ended: Mutex<HashMap<u64, Value>>,
    /// Whoever waits for a call that has not ended.
    waiting: Mutex<HashMap<u64, Waker>>,
    /// Set when something can make progress: a call ended, a future was
    /// woken, or the library asked for a turn.
    ready: Mutex<bool>,
    signal: Condvar,
    /// Each wake from the library: the thread it came from.
    wakes: Mutex<Vec<String>>,
}

impl Shared {
    fn notify(&self) {
        *self.ready.lock().expect("no panic") = true;
        self.signal.notify_one();
    }
}

static SHARED: std::sync::OnceLock<Arc<Shared>> = std::sync::OnceLock::new();

unsafe extern "C" fn on_done(_ctx: *mut c_void, call: u64, tag: u64, payload: u64) {
    let shared = SHARED.get().expect("set before init");
    shared
        .ended
        .lock()
        .expect("no panic")
        .insert(call, Value { tag, payload });
    if let Some(waker) = shared.waiting.lock().expect("no panic").remove(&call) {
        waker.wake();
    }
}

unsafe extern "C" fn wake(_ctx: *mut c_void) {
    let mut name = [0 as c_char; 64];
    // SAFETY: a buffer of its length.
    unsafe { pthread_getname_np(pthread_self(), name.as_mut_ptr(), name.len()) };
    // SAFETY: NUL-terminated by the call.
    let name = unsafe { std::ffi::CStr::from_ptr(name.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    let shared = SHARED.get().expect("set before init");
    shared.wakes.lock().expect("no panic").push(name);
    shared.notify();
}

/// A task of the executor.
type Job<'a> = Pin<Box<dyn Future<Output = ()> + 'a>>;

/// A call in flight, as a future of its value or of what it threw.
struct Call {
    id: u64,
}

impl Future for Call {
    type Output = Result<Value, Value>;
    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let shared = SHARED.get().expect("set");
        if let Some(value) = shared.ended.lock().expect("no panic").remove(&self.id) {
            return Poll::Ready(if value.tag & THROWN == 0 {
                Ok(value)
            } else {
                Err(Value {
                    tag: value.tag & !THROWN,
                    payload: value.payload,
                })
            });
        }
        shared
            .waiting
            .lock()
            .expect("no panic")
            .insert(self.id, context.waker().clone());
        Poll::Pending
    }
}

/// Start the export `name` with `args`.
fn call(lib: &Lib, name: &str, args: &[Value]) -> Call {
    // SAFETY: the library's contract; not during a turn.
    let id = unsafe {
        let index = (lib.find)(name.as_ptr(), name.len());
        assert!(index >= 0, "no export {name}");
        (lib.call_start)(index.cast_unsigned(), args.as_ptr(), args.len())
    };
    assert!(id != 0, "{name} refused its arguments");
    Call { id }
}

fn int(value: i64) -> Value {
    Value {
        tag: TAG_INT,
        payload: value.cast_unsigned(),
    }
}

/// A new BAML string, as a handle the host owns.
fn string(lib: &Lib, text: &str) -> Value {
    // SAFETY: not during a turn.
    let handle = unsafe { (lib.string_new)(text.as_ptr(), text.len()) };
    Value {
        tag: TAG_STRING | HANDLE,
        payload: handle,
    }
}

struct Flag(Arc<Shared>);
impl Wake for Flag {
    fn wake(self: Arc<Self>) {
        self.0.notify();
    }
}

/// The executor: poll every future that is not done, give BAML a turn,
/// and sleep until a call ends, the library asks for a turn or the next
/// BAML timer fires. Returns how many turns it gave.
fn run(lib: &Lib, futures: &mut [Job<'_>]) -> usize {
    let shared = Arc::clone(SHARED.get().expect("set"));
    let waker = Waker::from(Arc::new(Flag(Arc::clone(&shared))));
    let mut context = Context::from_waker(&waker);
    let mut done = vec![false; futures.len()];
    let mut turns = 0;
    loop {
        for (future, done) in futures.iter_mut().zip(&mut done) {
            if !*done && future.as_mut().poll(&mut context).is_ready() {
                *done = true;
            }
        }
        if done.iter().all(|done| *done) {
            return turns;
        }
        *shared.ready.lock().expect("no panic") = false;
        // SAFETY: on the thread that runs every turn, with nothing else of
        // the library running.
        let next_timer = unsafe { (lib.turn)() };
        turns += 1;
        let mut ready = shared.ready.lock().expect("no panic");
        if !*ready && shared.ended.lock().expect("no panic").is_empty() {
            ready = match u64::try_from(next_timer) {
                Ok(nanos) => {
                    shared
                        .signal
                        .wait_timeout_while(ready, Duration::from_nanos(nanos), |ready| !*ready)
                        .expect("no panic")
                        .0
                }
                Err(_) => shared
                    .signal
                    .wait_while(ready, |ready| !*ready)
                    .expect("no panic"),
            };
        }
        drop(ready);
    }
}

fn show(lib: &Lib, value: Value) -> String {
    if value.tag & HANDLE != 0 {
        let what = if value.tag & TAG_TYPE_ID == TAG_STRING {
            format!("{:?}", text(lib.string_copy, value.payload))
        } else {
            text(lib.describe, value.payload)
        };
        format!(
            "{what} (tag {} | HANDLE, handle {})",
            value.tag & TAG_TYPE_ID,
            value.payload
        )
    } else if value.tag == TAG_INT {
        format!("{} (tag INT)", value.payload.cast_signed())
    } else {
        format!("{value:?}")
    }
}

static SUM: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

unsafe extern "C" fn sum_done(_ctx: *mut c_void, _call: u64, _tag: u64, payload: u64) {
    SUM.fetch_add(payload, std::sync::atomic::Ordering::Relaxed);
}

unsafe extern "C" fn ignore_wake(_ctx: *mut c_void) {}

/// `add(i, 1)` for each `i` below `n`, through the bare surface.
fn bench(lib: &Lib, n: u64, batch: bool) {
    // SAFETY: the library's contract, on this thread alone.
    unsafe {
        assert_eq!((lib.init)(sum_done, ignore_wake, std::ptr::null_mut()), 0);
        let add = (lib.find)(b"add".as_ptr(), 3).cast_unsigned();
        let started = Instant::now();
        for i in 0..n {
            let args = [int(i.cast_signed()), int(1)];
            assert!((lib.call_start)(add, args.as_ptr(), 2) != 0);
            if !batch {
                (lib.turn)();
            }
        }
        if batch {
            (lib.turn)();
        }
        let elapsed = started.elapsed();
        let sum = SUM.load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(sum, n * (n + 1) / 2, "every call reported");
        #[expect(clippy::cast_precision_loss, reason = "a rate")]
        let each = elapsed.as_nanos() as f64 / n as f64;
        println!(
            "{n} calls of add, {}: {elapsed:?}, {each:.1} ns per call",
            if batch { "batch" } else { "serial" }
        );
    }
}

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: native_host <library.dylib>");
        std::process::exit(2);
    };
    let lib = match Lib::open(&path) {
        Ok(lib) => lib,
        Err(error) => {
            eprintln!("cannot load {path}: {error}");
            std::process::exit(1);
        }
    };
    let arguments: Vec<String> = std::env::args().collect();
    if let [_, _, flag, n, mode] = arguments.as_slice()
        && flag == "--bench"
    {
        bench(&lib, n.parse().expect("a count"), mode == "batch");
        return;
    }
    let _ = SHARED.set(Arc::new(Shared::default()));
    // SAFETY: once, before anything else of the library.
    let status = unsafe { (lib.init)(on_done, wake, std::ptr::null_mut()) };
    assert_eq!(status, 0, "init");

    // SAFETY: the library's contract.
    unsafe {
        println!("interface hash {:016x}", (lib.interface_hash)());
        for index in 0..(lib.export_count)() {
            let (mut arity, mut hash, mut text) = (0, 0, [0_u8; 128]);
            let length =
                (lib.export_info)(index, &raw mut arity, &raw mut hash, text.as_mut_ptr(), 128);
            let text = String::from_utf8_lossy(&text[..usize::try_from(length).expect("ok")]);
            println!("  export {index}: {text}  (arity {arity}, hash {hash:016x})");
        }
    }

    // 1. add(2, 40).
    let started = Instant::now();
    let mut futures: Vec<Job<'_>> = vec![Box::pin(async {
        let value = call(&lib, "add", &[int(2), int(40)]).await;
        println!("add(2, 40) = {}", show(&lib, value.expect("add returns")));
    })];
    let turns = run(&lib, &mut futures);
    println!("  {turns} turn(s), {:?}", started.elapsed());

    // 2. Three calls in flight at once: one parks on the timer, one on the
    // I/O thread, one does not park. They end in the order they can.
    let started = Instant::now();
    let order = Mutex::new(Vec::new());
    let home = string(&lib, "HOME");
    let missing = string(&lib, "BAML_BRIDGE_NO_SUCH_VARIABLE");
    let fallback = string(&lib, "(unset)");
    let mut futures: Vec<Job<'_>> = vec![
        Box::pin(async {
            let value = call(&lib, "nap", &[int(30)]).await;
            println!(
                "  nap(30) = {} after {:?}",
                show(&lib, value.expect("nap")),
                started.elapsed()
            );
            order.lock().expect("no panic").push("nap");
        }),
        Box::pin(async {
            let value = call(&lib, "env_or", &[home, fallback]).await;
            println!(
                "  env_or(\"HOME\", ..) = {} after {:?}",
                show(&lib, value.expect("env_or")),
                started.elapsed()
            );
            order.lock().expect("no panic").push("env_or HOME");
        }),
        Box::pin(async {
            let value = call(&lib, "env_or", &[missing, fallback]).await;
            println!(
                "  env_or(unset, ..) = {} after {:?}",
                show(&lib, value.expect("env_or")),
                started.elapsed()
            );
            order.lock().expect("no panic").push("env_or unset");
        }),
        Box::pin(async {
            let value = call(&lib, "add", &[int(1), int(1)]).await;
            println!(
                "  add(1, 1) = {} after {:?}",
                show(&lib, value.expect("add")),
                started.elapsed()
            );
            order.lock().expect("no panic").push("add");
        }),
    ];
    println!("four calls in flight:");
    let turns = run(&lib, &mut futures);
    drop(futures);
    let wakes = SHARED
        .get()
        .expect("set")
        .wakes
        .lock()
        .expect("no panic")
        .clone();
    println!(
        "  ended in order {:?}; {turns} turns; wakes from threads {wakes:?}",
        order.lock().expect("no panic")
    );

    // 3. A string in and out, the result a handle that survives a
    // collection which moves its object.
    let name = string(&lib, "native");
    let mut result = None;
    let mut futures: Vec<Job<'_>> = vec![Box::pin(async {
        result = Some(call(&lib, "greet", &[name]).await.expect("greet"));
    })];
    run(&lib, &mut futures);
    drop(futures);
    let result = result.expect("ran");
    // SAFETY: not during a turn.
    unsafe {
        let mut addresses = vec![(lib.debug_address)(result.payload)];
        for _ in 0..2 {
            (lib.collect)();
            addresses.push((lib.debug_address)(result.payload));
        }
        let addresses: Vec<String> = addresses
            .iter()
            .map(|address| format!("{address:#x}"))
            .collect();
        println!(
            "greet(\"native\") = {}; its object at {} across two collections",
            show(&lib, result),
            addresses.join(" -> ")
        );
        for handle in [
            home.payload,
            missing.payload,
            fallback.payload,
            name.payload,
            result.payload,
        ] {
            (lib.handle_free)(handle);
        }
    }

    // 4. A timer alone: how late the host's wait wakes BAML's timer.
    for milliseconds in [5, 100] {
        let started = Instant::now();
        let mut futures: Vec<Job<'_>> = vec![Box::pin(async {
            let value = call(&lib, "nap", &[int(milliseconds)]).await;
            value.expect("nap");
        })];
        let turns = run(&lib, &mut futures);
        println!(
            "nap({milliseconds}) alone: ended after {:?}, {turns} turns",
            started.elapsed()
        );
    }

    // 5. A call that throws.
    let message = string(&lib, "no luck");
    let mut futures: Vec<Job<'_>> = vec![Box::pin(async {
        match call(&lib, "fail", &[message]).await {
            Ok(value) => println!("fail returned {}", show(&lib, value)),
            Err(error) => println!("fail(\"no luck\") threw {}", show(&lib, error)),
        }
    })];
    run(&lib, &mut futures);
}
