//! The library form, called the way a host program calls it: the program
//! in `tests/host_lib/fixture.baml` compiled into a shared library, loaded
//! into this process with `dlopen` and driven through its `baml_lib_*`
//! functions alone.
//!
//! One library per process: the tests share it and take turns.
#![cfg(all(target_arch = "aarch64", target_vendor = "apple"))]
#![allow(
    unsafe_code,
    clippy::missing_transmute_annotations,
    reason = "a C interface found by name"
)]

use std::{
    collections::HashMap,
    ffi::{CString, c_char, c_int, c_void},
    path::Path,
    sync::{Mutex, MutexGuard, OnceLock},
    time::{Duration, Instant},
};

use baml_compiler2_native::{CompileOptions, compile_library, link_library};
use baml_db::{
    ProjectDatabase, SourceRootKind, SourceRootSpec,
    baml_compiler2_hir::item_data::{file_functions, function_data},
};

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Value {
    tag: u64,
    payload: u64,
}

const TAG_INT: u64 = 2;
const TAG_OMITTED: u64 = 4;
const TAG_STRING: u64 = 5;
const TAG_STRING_SLICE: u64 = 6;
const THROWN: u64 = 1 << 59;
const HANDLE: u64 = 1 << 61;
const BUSY: u64 = 1 << 63;

const STATUS_RETURNED: u32 = 0;
const STATUS_THREW: u32 = 1;
const STATUS_STUCK: u32 = 2;
const STATUS_PANICKED: u32 = 3;

#[repr(C)]
struct Callbacks {
    size: u32,
    ctx: *mut c_void,
    on_done: unsafe extern "C" fn(*mut c_void, u64, u32, u64, u64),
    on_unhandled: unsafe extern "C" fn(*mut c_void, u64, u64, u64),
    wake: unsafe extern "C" fn(*mut c_void),
}

unsafe extern "C" {
    fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

struct Lib {
    find: unsafe extern "C" fn(*const u8, usize) -> i64,
    call_start: unsafe extern "C" fn(u64, *const Value, usize) -> u64,
    turn: unsafe extern "C" fn() -> i64,
    string_new: unsafe extern "C" fn(*const u8, usize) -> u64,
    string_copy: unsafe extern "C" fn(u64, *mut u8, usize) -> i64,
    handle_free: unsafe extern "C" fn(u64),
    collect: unsafe extern "C" fn(),
    describe: unsafe extern "C" fn(u64, *mut u8, usize) -> i64,
    task_count: unsafe extern "C" fn() -> u64,
    array_len: unsafe extern "C" fn(u64) -> i64,
    last_error: unsafe extern "C" fn(*mut u8, usize) -> i64,
}

/// How a call ended: its status and value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Ending {
    status: u32,
    value: Value,
}

/// What `on_done` reported, by call.
static ENDED: Mutex<Option<HashMap<u64, Ending>>> = Mutex::new(None);
/// What `on_unhandled` reported: the call, and the error.
static UNHANDLED: Mutex<Vec<(u64, Value)>> = Mutex::new(Vec::new());

unsafe extern "C" fn on_done(_ctx: *mut c_void, call: u64, status: u32, tag: u64, payload: u64) {
    let ending = Ending {
        status,
        value: Value { tag, payload },
    };
    let earlier = ENDED
        .lock()
        .expect("no test panicked holding it")
        .get_or_insert_default()
        .insert(call, ending);
    assert!(earlier.is_none(), "call {call} ended twice");
}

unsafe extern "C" fn on_unhandled(_ctx: *mut c_void, call: u64, tag: u64, payload: u64) {
    UNHANDLED
        .lock()
        .expect("no test panicked holding it")
        .push((call, Value { tag, payload }));
}

unsafe extern "C" fn wake(_ctx: *mut c_void) {}

/// The fixture as a library, compiled, linked, loaded and initialized once.
fn lib() -> &'static Lib {
    static LIB: OnceLock<Lib> = OnceLock::new();
    LIB.get_or_init(|| {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/host_lib");
        let path = directory.join("fixture.baml");
        let text = std::fs::read_to_string(&path).expect("the fixture");
        let mut db = ProjectDatabase::new();
        db.ensure_stdlib_sources();
        let root = db
            .add_source_root(SourceRootSpec::new(&directory, SourceRootKind::Workspace))
            .expect("a source root");
        db.add_or_update_file_in(root, &path, &text);
        let exports: Vec<_> = db
            .workspace_files()
            .into_iter()
            .flat_map(|file| file_functions(&db, file).iter().copied())
            .filter(|function| function_data(&db, *function).name.as_str() != "boom")
            .collect();
        let compiled = compile_library(&db, &exports, CompileOptions::default())
            .unwrap_or_else(|error| panic!("the fixture compiles: {error}"));
        let out = tempfile::tempdir().expect("a temporary directory").keep();
        let library = out.join("libfixture.dylib");
        link_library(&compiled.object, &library).expect("the fixture links");
        let library = CString::new(library.to_str().expect("UTF-8")).expect("a path");
        // SAFETY: a library with the `baml_lib_*` functions of these types.
        unsafe {
            let handle = dlopen(library.as_ptr(), 2);
            assert!(!handle.is_null(), "the library loads");
            let symbol = |name: &str| {
                let name = CString::new(name).expect("a name");
                let address = dlsym(handle, name.as_ptr());
                assert!(!address.is_null(), "{name:?} is exported");
                address
            };
            let init: unsafe extern "C" fn(*const Callbacks) -> i32 =
                std::mem::transmute(symbol("baml_lib_init"));
            let mut callbacks = Callbacks {
                size: 0,
                ctx: std::ptr::null_mut(),
                on_done,
                on_unhandled,
                wake,
            };
            assert_eq!(
                init(&raw const callbacks),
                -2,
                "a struct too small is refused"
            );
            callbacks.size = u32::try_from(size_of::<Callbacks>()).expect("small");
            assert_eq!(init(&raw const callbacks), 0);
            assert_eq!(init(&raw const callbacks), -1, "init runs once");
            Lib {
                find: std::mem::transmute(symbol("baml_lib_find")),
                call_start: std::mem::transmute(symbol("baml_lib_call_start")),
                turn: std::mem::transmute(symbol("baml_lib_turn")),
                string_new: std::mem::transmute(symbol("baml_lib_string_new")),
                string_copy: std::mem::transmute(symbol("baml_lib_string_copy")),
                handle_free: std::mem::transmute(symbol("baml_lib_handle_free")),
                collect: std::mem::transmute(symbol("baml_lib_collect")),
                describe: std::mem::transmute(symbol("baml_lib_describe")),
                task_count: std::mem::transmute(symbol("baml_lib_debug_task_count")),
                array_len: std::mem::transmute(symbol("baml_lib_array_len")),
                last_error: std::mem::transmute(symbol("baml_lib_last_error")),
            }
        }
    })
}

/// The library, for one test at a time.
fn take_turns() -> (MutexGuard<'static, ()>, &'static Lib) {
    static TURNS: Mutex<()> = Mutex::new(());
    let guard = TURNS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    (guard, lib())
}

fn int(value: i64) -> Value {
    Value {
        tag: TAG_INT,
        payload: value.cast_unsigned(),
    }
}

impl Lib {
    fn export(&self, name: &str) -> u64 {
        // SAFETY: a name.
        let index = unsafe { (self.find)(name.as_ptr(), name.len()) };
        u64::try_from(index).unwrap_or_else(|_| panic!("no export {name}"))
    }

    /// Start `name`; zero when the library refuses the arguments.
    fn start(&self, name: &str, args: &[Value]) -> u64 {
        // SAFETY: `args` is `args.len()` values.
        unsafe { (self.call_start)(self.export(name), args.as_ptr(), args.len()) }
    }

    /// Turn until call `call` has ended, and return what it returned.
    fn finish(&self, call: u64) -> Value {
        let ending = self.end(call);
        assert_eq!(ending.status, STATUS_RETURNED, "{ending:?}");
        ending.value
    }

    /// Turn until call `call` has ended, and say how.
    fn end(&self, call: u64) -> Ending {
        assert_ne!(call, 0, "the call started");
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            // SAFETY: one test at a time.
            unsafe { (self.turn)() };
            let ended = ENDED
                .lock()
                .expect("no test panicked holding it")
                .get_or_insert_default()
                .remove(&call);
            if let Some(value) = ended {
                return value;
            }
            assert!(Instant::now() < deadline, "call {call} never ended");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn string(&self, text: &str) -> Value {
        // SAFETY: `text` is its length of bytes.
        let handle = unsafe { (self.string_new)(text.as_ptr(), text.len()) };
        Value {
            tag: TAG_STRING | HANDLE,
            payload: handle,
        }
    }

    /// Turn until `on_unhandled` has reported something, and take it.
    fn unhandled(&self) -> (u64, Value) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            // SAFETY: one test at a time.
            unsafe { (self.turn)() };
            if let Some(reported) = UNHANDLED.lock().expect("no test panicked").pop() {
                return reported;
            }
            assert!(Instant::now() < deadline, "nothing was reported unhandled");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn tasks(&self) -> u64 {
        // SAFETY: one test at a time.
        unsafe { (self.task_count)() }
    }

    fn describe(&self, handle: u64) -> String {
        let mut buffer = vec![0_u8; 256];
        // SAFETY: a buffer of its length.
        let length = unsafe { (self.describe)(handle, buffer.as_mut_ptr(), buffer.len()) };
        buffer.truncate(usize::try_from(length).expect("a live handle"));
        String::from_utf8(buffer).expect("UTF-8")
    }

    fn last_error(&self) -> String {
        let mut buffer = vec![0_u8; 256];
        // SAFETY: a buffer of its length.
        let length = unsafe { (self.last_error)(buffer.as_mut_ptr(), buffer.len()) };
        buffer.truncate(usize::try_from(length).expect("a length"));
        String::from_utf8(buffer).expect("UTF-8")
    }

    fn text(&self, handle: u64) -> String {
        let mut buffer = vec![0_u8; 256];
        // SAFETY: a buffer of its length.
        let length = unsafe { (self.string_copy)(handle, buffer.as_mut_ptr(), buffer.len()) };
        buffer.truncate(usize::try_from(length).expect("a string"));
        String::from_utf8(buffer).expect("UTF-8")
    }

    fn free(&self, value: Value) {
        if value.tag & HANDLE != 0 {
            // SAFETY: a handle this test owns.
            unsafe { (self.handle_free)(value.payload) };
        }
    }
}

/// Review finding 1: a string handle is a string whatever its form. A part
/// of a string comes back as a slice; the host passes it back as a string,
/// and after a collection that made it a string, as the slice it was told.
#[test]
fn a_string_handle_is_accepted_as_any_string() {
    let (_turn, lib) = take_turns();
    let long = lib.string("a string long enough that a part of it is a slice");
    let tail = lib.finish(lib.start("tail", &[long]));
    assert_eq!(tail.tag, TAG_STRING_SLICE | HANDLE, "the part is a slice");
    assert_eq!(
        lib.text(tail.payload),
        " string long enough that a part of it is a slice"
    );
    for passed_as in [TAG_STRING | HANDLE, TAG_STRING_SLICE | HANDLE] {
        let arg = Value {
            tag: passed_as,
            payload: tail.payload,
        };
        let length = lib.finish(lib.start("length_of", &[arg]));
        assert_eq!(length, int(48));
        // The slice's parent dies: the collection makes the slice a string.
        lib.free(long);
        // SAFETY: one test at a time.
        unsafe { (lib.collect)() };
    }
    lib.free(tail);
}

/// Review finding 4: no arguments may come as a null pointer, and a
/// buffer of no bytes may be null, to ask for a length.
#[test]
fn null_pointers_with_no_length_are_accepted() {
    let (_turn, lib) = take_turns();
    // SAFETY: no values at a null pointer.
    let call = unsafe { (lib.call_start)(lib.export("seven"), std::ptr::null(), 0) };
    assert_eq!(lib.finish(call), int(7));
    let text = lib.string("hello");
    // SAFETY: no bytes at a null pointer.
    let length = unsafe { (lib.string_copy)(text.payload, std::ptr::null_mut(), 0) };
    assert_eq!(length, 5);
    lib.free(text);
}

/// Review finding 5: a scalar's tag with any bit set beyond its type, and
/// an argument left to its default, are refused and never stored.
#[test]
fn tags_with_stray_bits_are_refused() {
    let (_turn, lib) = take_turns();
    for bad in [
        Value {
            tag: TAG_INT | BUSY,
            payload: 1,
        },
        Value {
            tag: TAG_INT | THROWN,
            payload: 1,
        },
        Value {
            tag: TAG_OMITTED,
            payload: 0,
        },
        Value {
            tag: TAG_STRING | HANDLE | BUSY,
            payload: 0,
        },
    ] {
        assert_eq!(lib.start("add", &[bad, int(2)]), 0, "{bad:?} is refused");
    }
    assert_eq!(lib.finish(lib.start("add", &[int(1), int(2)])), int(3));
}

/// Review finding 2: a call that can never finish is reported, once, and
/// is over for the host.
#[test]
fn a_call_that_can_never_finish_is_reported_stuck() {
    let (_turn, lib) = take_turns();
    let ending = lib.end(lib.start("stuck", &[]));
    assert_eq!(
        ending,
        Ending {
            status: STATUS_STUCK,
            value: Value { tag: 0, payload: 0 },
        }
    );
    // Later turns report nothing more for it, and other calls still run.
    assert_eq!(lib.finish(lib.start("add", &[int(2), int(3)])), int(5));
}

/// Review finding 3: a spawned task that fails with nothing awaiting it is
/// reported once its call has finished, and freed.
#[test]
fn a_failure_nobody_awaits_is_reported_and_freed() {
    let (_turn, lib) = take_turns();
    let before = lib.tasks();
    let call = lib.start("fails_unawaited", &[]);
    assert_eq!(lib.finish(call), int(1));
    let (from, error) = lib.unhandled();
    assert_eq!(from, call);
    assert_eq!(error.tag, TAG_STRING | HANDLE);
    assert_eq!(lib.text(error.payload), "boom");
    lib.free(error);
    assert_eq!(lib.tasks(), before, "the failed task is freed");

    // One that fails after its call returned is reported when it fails.
    let call = lib.start("fails_after_return", &[]);
    assert_eq!(lib.finish(call), int(2));
    let (from, error) = lib.unhandled();
    assert_eq!(from, call);
    assert_eq!(lib.text(error.payload), "boom");
    lib.free(error);
    assert_eq!(lib.tasks(), before);

    // One that is awaited is not reported.
    assert_eq!(lib.finish(lib.start("fails_awaited", &[])), int(30));
    assert!(UNHANDLED.lock().expect("no test panicked").is_empty());
    assert_eq!(lib.tasks(), before);
}

/// `baml.panics.Exit` never ends the host's process: from the call's own
/// task it is a panic, from a spawned task a failure nobody awaits.
#[test]
fn an_exit_does_not_end_the_host() {
    let (_turn, lib) = take_turns();
    let ending = lib.end(lib.start("quits", &[int(3)]));
    assert_eq!(ending.status, STATUS_PANICKED, "{ending:?}");
    assert!(
        lib.describe(ending.value.payload).contains("Exit"),
        "{}",
        lib.describe(ending.value.payload)
    );
    lib.free(ending.value);
    let call = lib.start("quits_in_a_task", &[int(4)]);
    assert_eq!(lib.finish(call), int(5));
    let (from, error) = lib.unhandled();
    assert_eq!(from, call);
    assert!(lib.describe(error.payload).contains("Exit"));
    lib.free(error);
}

/// A call that throws says so, apart from a panic.
#[test]
fn a_throw_is_told_from_a_return() {
    let (_turn, lib) = take_turns();
    let ending = lib.end(lib.start("boom_export", &[int(1)]));
    assert_eq!(ending.status, STATUS_THREW, "{ending:?}");
    assert_eq!(lib.text(ending.value.payload), "boom");
    lib.free(ending.value);
}

/// `OMITTED` is taken for a parameter with a default, and refused for one
/// without.
#[test]
fn omitted_is_taken_where_a_default_is_declared() {
    let (_turn, lib) = take_turns();
    let omitted = Value {
        tag: TAG_OMITTED,
        payload: 0,
    };
    let greeting = lib.finish(lib.start("greet_default", &[omitted]));
    assert_eq!(lib.text(greeting.payload), "hi you");
    lib.free(greeting);
    assert_eq!(lib.start("add", &[omitted, int(1)]), 0);
    assert!(lib.last_error().contains("OMITTED"), "{}", lib.last_error());
}

/// A handle of the wrong kind is refused, with a reason, never read as
/// what it is not.
#[test]
fn a_handle_of_the_wrong_kind_is_refused() {
    let (_turn, lib) = take_turns();
    let text = lib.string("not an array");
    // SAFETY: a live handle, outside a turn.
    assert_eq!(unsafe { (lib.array_len)(text.payload) }, -1);
    assert!(lib.last_error().contains("not 10"), "{}", lib.last_error());
    lib.free(text);
    // SAFETY: as above.
    assert_eq!(
        unsafe { (lib.array_len)(text.payload) },
        -1,
        "a freed handle"
    );
    // SAFETY: as above.
    assert_eq!(unsafe { (lib.array_len)(u64::MAX) }, -1, "no such handle");
}
