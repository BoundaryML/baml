//! Calling a BAML program compiled to a native library from Rust.
//!
//! `native_pack --lib` makes a shared library that exports the C functions
//! of `baml_native_rt/src/sched/host.rs` (`baml_native_rt/design/bridges.md`).
//! This crate is what generated Rust bindings sit on:
//!
//! - [`Library::load`] opens the library, checks its interface hash against
//!   the bindings' and builds its worker. One library per process.
//! - [`Library::call`] starts an exported function and returns a [`Call`],
//!   a future of its result. Any executor drives it: polling a call runs
//!   the library's turns when it asked for one, on the polling thread, one
//!   thread at a time.
//! - [`ToBaml`] and [`FromBaml`] copy a Rust value into BAML memory and out
//!   of it. Generated structs implement them with the class and field
//!   numbers of the library's interface (`native_pack --lib --describe`).
//!
//! User code never holds a handle: a [`BamlValue`] lives only inside a
//! [`Session`], the lock every call into the library takes, and frees its
//! handle when dropped.

#![expect(
    unsafe_code,
    clippy::missing_transmute_annotations,
    reason = "a C interface found by name in a library loaded at run time, each function \
              transmuted to the type of the field it is kept in"
)]

use std::{
    collections::{HashMap, HashSet},
    ffi::{CString, c_char, c_int, c_void},
    future::Future,
    marker::PhantomData,
    pin::Pin,
    sync::{
        Arc, Condvar, Mutex, MutexGuard, OnceLock, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};

/// The map type generated bindings use, as today's Rust SDK does.
pub use indexmap::IndexMap as Map;

/// A value as it crosses the C boundary: a tag and a payload.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawValue {
    pub tag: u64,
    pub payload: u64,
}

/// The tags a binding tests, as `baml_native_rt`'s `layout.rs` has them.
pub mod tag {
    pub const NULL: u64 = 0;
    pub const BOOL: u64 = 1;
    pub const INT: u64 = 2;
    pub const FLOAT: u64 = 3;
    pub const OMITTED: u64 = 4;
    pub const STRING: u64 = 5;
    pub const STRING_SLICE: u64 = 6;
    pub const INSTANCE: u64 = 7;
    pub const ENUM_VARIANT: u64 = 8;
    pub const ARRAY: u64 = 10;
    pub const MAP: u64 = 12;
    /// Set in the tag of an object: the payload is a handle.
    pub const HANDLE: u64 = 1 << 61;
    /// The bits that are the type.
    pub const TYPE: u64 = 0xFFFF_FFFF;
}

/// The version of the C interface this crate speaks.
const ABI_VERSION: u32 = 2;

/// How a call ended, as the library says.
const STATUS_RETURNED: u32 = 0;
const STATUS_THREW: u32 = 1;
const STATUS_STUCK: u32 = 2;
const STATUS_PANICKED: u32 = 3;

/// What went wrong on this side of the boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SdkError {
    /// The library could not be loaded or does not match the bindings.
    Load(String),
    /// The library refused a value or a call: its reason.
    Refused(String),
    /// A value did not have the shape the bindings expected.
    Decode(String),
}

impl std::fmt::Display for SdkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Load(why) => write!(f, "cannot load the BAML library: {why}"),
            Self::Refused(why) => write!(f, "the BAML library refused: {why}"),
            Self::Decode(why) => write!(f, "unexpected value from BAML: {why}"),
        }
    }
}

impl std::error::Error for SdkError {}

/// How a call failed. `E` is the function's declared `throws` type.
#[derive(Clone, Debug, PartialEq)]
pub enum Error<E = std::convert::Infallible> {
    /// It threw a value of its declared `throws` type.
    Thrown {
        value: Box<E>,
    },
    /// It threw something else: the class's name when it is an instance,
    /// and the value as text.
    Runtime {
        class_name: Option<String>,
        message: String,
    },
    /// It panicked (`baml.panics`, `baml.panics.Exit` included).
    Panic {
        message: String,
    },
    /// It can never finish: nothing can run, no timer is set, and no I/O
    /// can complete.
    Stuck,
    Sdk(SdkError),
}

impl<E: std::fmt::Debug> std::fmt::Display for Error<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Thrown { value } => write!(f, "BAML threw {value:?}"),
            Self::Runtime { message, .. } => write!(f, "BAML threw {message}"),
            Self::Panic { message } => write!(f, "BAML panicked: {message}"),
            Self::Stuck => write!(f, "the BAML call can never finish"),
            Self::Sdk(error) => write!(f, "{error}"),
        }
    }
}

impl<E: std::fmt::Debug> std::error::Error for Error<E> {}

impl<E> From<SdkError> for Error<E> {
    fn from(error: SdkError) -> Self {
        Self::Sdk(error)
    }
}

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
    fn dlerror() -> *const c_char;
}

/// The library's C functions.
struct Fns {
    find: unsafe extern "C" fn(*const u8, usize) -> i64,
    interface_hash: unsafe extern "C" fn() -> u64,
    call_start: unsafe extern "C" fn(u64, *const RawValue, usize) -> u64,
    turn: unsafe extern "C" fn() -> i64,
    last_error: unsafe extern "C" fn(*mut u8, usize) -> i64,
    handle_free: unsafe extern "C" fn(u64),
    describe: unsafe extern "C" fn(u64, *mut u8, usize) -> i64,
    collect: unsafe extern "C" fn(),
    handle_count: unsafe extern "C" fn() -> u64,
    string_new: unsafe extern "C" fn(*const u8, usize) -> u64,
    string_copy: unsafe extern "C" fn(u64, *mut u8, usize) -> i64,
    class_of: unsafe extern "C" fn(u64) -> i64,
    field_get: unsafe extern "C" fn(u64, u32, *mut RawValue) -> i32,
    array_len: unsafe extern "C" fn(u64) -> i64,
    array_get: unsafe extern "C" fn(u64, u64, *mut RawValue) -> i32,
    map_next: unsafe extern "C" fn(u64, u64, *mut RawValue, *mut RawValue) -> i64,
    variant_index: unsafe extern "C" fn(u64) -> i64,
    variant: unsafe extern "C" fn(u32, u32, *mut RawValue) -> i32,
    instance_new: unsafe extern "C" fn(u32, *const RawValue, usize, *mut i32) -> u64,
    array_new: unsafe extern "C" fn(*const RawValue, usize) -> u64,
    map_new: unsafe extern "C" fn(*const RawValue, *const RawValue, usize) -> u64,
}

/// What the library's callbacks leave for the calls, under its own lock,
/// which a callback can take while the turn lock is held.
#[derive(Default)]
struct Inbox {
    ended: HashMap<u64, (u32, RawValue)>,
    waiting: HashMap<u64, Waker>,
    /// Calls whose `Call` was dropped: their results are freed.
    abandoned: HashSet<u64>,
    /// Handles to free at the next turn lock.
    frees: Vec<u64>,
    /// Errors of tasks nobody awaited, for the hook.
    unhandled: Vec<RawValue>,
}

/// When the library's next timer fires, for the timer thread.
#[derive(Default)]
struct Timer {
    deadline: Mutex<Option<Instant>>,
    changed: Condvar,
}

type Hook = Box<dyn Fn(&str) + Send + Sync>;

/// A loaded library. One per process.
pub struct Library {
    fns: Fns,
    /// Every call into the library holds it: one thread at a time, and
    /// never two at once.
    lock: Mutex<()>,
    inbox: Mutex<Inbox>,
    /// The library asked for a turn: a call started, a task was woken, or
    /// a timer fired.
    need_turn: AtomicBool,
    timer: Arc<Timer>,
    on_unhandled: RwLock<Hook>,
}

static LIBRARY: OnceLock<Library> = OnceLock::new();

/// What is done with the error of a task nobody awaited, unless the user
/// says otherwise.
#[expect(clippy::print_stderr, reason = "the default the user can replace")]
fn print_unhandled(error: &str) {
    eprintln!("baml: a task failed and nothing awaited it: {error}");
}

unsafe extern "C" fn on_done(_ctx: *mut c_void, call: u64, status: u32, tag: u64, payload: u64) {
    let Some(library) = LIBRARY.get() else { return };
    let value = RawValue { tag, payload };
    let waker = {
        let mut inbox = library
            .inbox
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if inbox.abandoned.remove(&call) {
            if tag & tag::HANDLE != 0 {
                inbox.frees.push(payload);
            }
            None
        } else {
            inbox.ended.insert(call, (status, value));
            inbox.waiting.remove(&call)
        }
    };
    if let Some(waker) = waker {
        waker.wake();
    }
}

unsafe extern "C" fn on_unhandled(_ctx: *mut c_void, _call: u64, tag: u64, payload: u64) {
    if let Some(library) = LIBRARY.get() {
        library
            .inbox
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .unhandled
            .push(RawValue { tag, payload });
    }
}

unsafe extern "C" fn wake(_ctx: *mut c_void) {
    if let Some(library) = LIBRARY.get() {
        library.wake_all();
    }
}

impl Library {
    /// Open the library at `path`, refuse it when its interface hash is not
    /// `expected_hash` (the bindings'), and build its worker. Loading again
    /// returns the library already loaded, if its hash matches.
    pub fn load(path: &str, expected_hash: u64) -> Result<&'static Self, SdkError> {
        if let Some(library) = LIBRARY.get() {
            return library.check_hash(expected_hash).map(|()| library);
        }
        let fns = open(path)?;
        let library = LIBRARY.get_or_init(|| Self {
            fns,
            lock: Mutex::new(()),
            inbox: Mutex::new(Inbox::default()),
            need_turn: AtomicBool::new(false),
            timer: Arc::new(Timer::default()),
            on_unhandled: RwLock::new(Box::new(print_unhandled)),
        });
        library.check_hash(expected_hash)?;
        let callbacks = Callbacks {
            size: u32::try_from(size_of::<Callbacks>()).expect("a small struct"),
            ctx: std::ptr::null_mut(),
            on_done,
            on_unhandled,
            wake,
        };
        let init: unsafe extern "C" fn(*const Callbacks) -> i32 =
            // SAFETY: the library exports it with this type.
            unsafe { std::mem::transmute::<*mut c_void, _>(symbol_of(path, "baml_lib_init")?) };
        let _guard = library.lock();
        // SAFETY: once per process, before any other call into the library.
        let status = unsafe { init(&raw const callbacks) };
        if status != 0 && status != -1 {
            return Err(SdkError::Load(format!("init failed with {status}")));
        }
        library.start_timer();
        Ok(library)
    }

    fn check_hash(&self, expected: u64) -> Result<(), SdkError> {
        // SAFETY: reads the library's table.
        let actual = unsafe { (self.fns.interface_hash)() };
        if actual == expected {
            Ok(())
        } else {
            Err(SdkError::Load(format!(
                "the library's interface hash is {actual:016x}, the bindings' {expected:016x}: \
                 the bindings were generated for another version of the program"
            )))
        }
    }

    /// Set what is done with the error of a task nobody awaited. The
    /// default prints it to stderr.
    pub fn set_on_unhandled(&self, hook: impl Fn(&str) + Send + Sync + 'static) {
        *self
            .on_unhandled
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Box::new(hook);
    }

    fn lock(&self) -> MutexGuard<'_, ()> {
        self.lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The library, for one thread: every call into it goes through a
    /// session. Handles freed while it was not held are freed now.
    pub fn session(&'static self) -> Session {
        let guard = self.lock();
        let frees = std::mem::take(
            &mut self
                .inbox
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .frees,
        );
        for handle in frees {
            // SAFETY: a handle this crate owned, under the lock.
            unsafe { (self.fns.handle_free)(handle) };
        }
        Session {
            library: self,
            _guard: guard,
        }
    }

    /// Wake every call that waits: one of them gives the library a turn.
    fn wake_all(&self) {
        self.need_turn.store(true, Ordering::Release);
        let wakers: Vec<Waker> = self
            .inbox
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .waiting
            .values()
            .cloned()
            .collect();
        for waker in wakers {
            waker.wake();
        }
    }

    /// The thread that wakes the calls when the library's next timer fires.
    fn start_timer(&'static self) {
        let timer = Arc::clone(&self.timer);
        std::thread::Builder::new()
            .name("baml-host-timer".to_owned())
            .spawn(move || {
                let mut deadline = timer
                    .deadline
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                loop {
                    match *deadline {
                        None => {
                            deadline = timer
                                .changed
                                .wait(deadline)
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                        }
                        Some(when) => {
                            let now = Instant::now();
                            if now >= when {
                                *deadline = None;
                                drop(deadline);
                                self.wake_all();
                                deadline = timer
                                    .deadline
                                    .lock()
                                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                            } else {
                                deadline = timer
                                    .changed
                                    .wait_timeout(deadline, when - now)
                                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                                    .0;
                            }
                        }
                    }
                }
            })
            .expect("the timer thread starts");
    }

    /// Start export `export` with the arguments `args` builds, and return
    /// the future of its result.
    pub fn call<T: FromBaml, E: FromBaml>(
        &'static self,
        export: u32,
        args: impl FnOnce(&Session) -> Result<Vec<BamlValue<'_>>, SdkError>,
    ) -> Call<T, E> {
        let session = self.session();
        let started = args(&session).and_then(|args| {
            let raw: Vec<RawValue> = args.iter().map(BamlValue::raw).collect();
            // SAFETY: `raw` is its length of values, under the lock.
            let id = unsafe { (self.fns.call_start)(u64::from(export), raw.as_ptr(), raw.len()) };
            drop(args);
            if id == 0 {
                Err(SdkError::Refused(session.last_error()))
            } else {
                Ok(id)
            }
        });
        drop(session);
        self.need_turn.store(true, Ordering::Release);
        match started {
            Ok(id) => Call {
                library: self,
                id,
                failed: None,
                done: false,
                result: PhantomData,
            },
            Err(error) => Call {
                library: self,
                id: 0,
                failed: Some(error),
                done: false,
                result: PhantomData,
            },
        }
    }

    /// The number of an export, by name: for bindings that check theirs.
    pub fn find(&'static self, name: &str) -> Option<u32> {
        let _session = self.session();
        // SAFETY: a name.
        let index = unsafe { (self.fns.find)(name.as_ptr(), name.len()) };
        u32::try_from(index).ok()
    }

    /// Collect now. For tests.
    pub fn collect(&'static self) {
        let _session = self.session();
        // SAFETY: under the lock.
        unsafe { (self.fns.collect)() };
    }

    /// How many handles are live. For tests.
    pub fn handle_count(&'static self) -> u64 {
        let _session = self.session();
        // SAFETY: under the lock.
        unsafe { (self.fns.handle_count)() }
    }
}

fn symbol_of(path: &str, name: &str) -> Result<*mut c_void, SdkError> {
    let path = CString::new(path).map_err(|error| SdkError::Load(error.to_string()))?;
    let name = CString::new(name).map_err(|error| SdkError::Load(error.to_string()))?;
    // SAFETY: a path and a name; dlopen of a loaded library returns it.
    unsafe {
        let handle = dlopen(path.as_ptr(), 2);
        if handle.is_null() {
            let why = std::ffi::CStr::from_ptr(dlerror())
                .to_string_lossy()
                .into_owned();
            return Err(SdkError::Load(why));
        }
        let address = dlsym(handle, name.as_ptr());
        if address.is_null() {
            return Err(SdkError::Load(format!("no symbol {name:?}")));
        }
        Ok(address)
    }
}

fn open(path: &str) -> Result<Fns, SdkError> {
    let version: unsafe extern "C" fn() -> u32 =
        // SAFETY: the library exports it with this type.
        unsafe { std::mem::transmute::<*mut c_void, _>(symbol_of(path, "baml_lib_abi_version")?) };
    // SAFETY: as above.
    let version = unsafe { version() };
    if version != ABI_VERSION {
        return Err(SdkError::Load(format!(
            "the library speaks version {version} of the C interface, this crate {ABI_VERSION}"
        )));
    }
    macro_rules! function {
        ($name:literal) => {
            // SAFETY: the library exports it with the field's type.
            unsafe { std::mem::transmute::<*mut c_void, _>(symbol_of(path, $name)?) }
        };
    }
    Ok(Fns {
        find: function!("baml_lib_find"),
        interface_hash: function!("baml_lib_interface_hash"),
        call_start: function!("baml_lib_call_start"),
        turn: function!("baml_lib_turn"),
        last_error: function!("baml_lib_last_error"),
        handle_free: function!("baml_lib_handle_free"),
        describe: function!("baml_lib_describe"),
        collect: function!("baml_lib_collect"),
        handle_count: function!("baml_lib_debug_handle_count"),
        string_new: function!("baml_lib_string_new"),
        string_copy: function!("baml_lib_string_copy"),
        class_of: function!("baml_lib_class_of"),
        field_get: function!("baml_lib_field_get"),
        array_len: function!("baml_lib_array_len"),
        array_get: function!("baml_lib_array_get"),
        map_next: function!("baml_lib_map_next"),
        variant_index: function!("baml_lib_variant_index"),
        variant: function!("baml_lib_variant"),
        instance_new: function!("baml_lib_instance_new"),
        array_new: function!("baml_lib_array_new"),
        map_new: function!("baml_lib_map_new"),
    })
}

/// The library held by one thread: what reads and builds values. A
/// [`BamlValue`] cannot outlive it.
pub struct Session {
    library: &'static Library,
    _guard: MutexGuard<'static, ()>,
}

impl Session {
    fn fns(&self) -> &Fns {
        &self.library.fns
    }

    /// Why the library refused the last thing it refused.
    pub fn last_error(&self) -> String {
        // SAFETY: under the lock; a buffer of its length.
        unsafe { text_from(|out, capacity| (self.fns().last_error)(out, capacity)) }
            .unwrap_or_default()
    }

    fn refused(&self) -> SdkError {
        SdkError::Refused(self.last_error())
    }

    fn owned(&self, raw: RawValue) -> BamlValue<'_> {
        BamlValue { raw, session: self }
    }

    /// A scalar.
    pub fn scalar(&self, tag: u64, payload: u64) -> BamlValue<'_> {
        self.owned(RawValue { tag, payload })
    }

    pub fn null(&self) -> BamlValue<'_> {
        self.scalar(tag::NULL, 0)
    }

    /// `OMITTED`: a parameter left to its default.
    pub fn omitted(&self) -> BamlValue<'_> {
        self.scalar(tag::OMITTED, 0)
    }

    pub fn int(&self, value: i64) -> BamlValue<'_> {
        self.scalar(tag::INT, value.cast_unsigned())
    }

    pub fn float(&self, value: f64) -> BamlValue<'_> {
        self.scalar(tag::FLOAT, value.to_bits())
    }

    pub fn bool(&self, value: bool) -> BamlValue<'_> {
        self.scalar(tag::BOOL, u64::from(value))
    }

    pub fn string(&self, text: &str) -> Result<BamlValue<'_>, SdkError> {
        // SAFETY: `text` is its length of bytes, under the lock.
        let handle = unsafe { (self.fns().string_new)(text.as_ptr(), text.len()) };
        self.handle(tag::STRING, handle)
    }

    fn handle(&self, kind: u64, handle: u64) -> Result<BamlValue<'_>, SdkError> {
        if handle == 0 {
            Err(self.refused())
        } else {
            Ok(self.owned(RawValue {
                tag: kind | tag::HANDLE,
                payload: handle,
            }))
        }
    }

    /// An instance of the class numbered `class`, its fields in declaration
    /// order. The library checks each against its field's type.
    pub fn instance(
        &self,
        class: u32,
        fields: Vec<BamlValue<'_>>,
    ) -> Result<BamlValue<'_>, SdkError> {
        let raw: Vec<RawValue> = fields.iter().map(BamlValue::raw).collect();
        // SAFETY: `raw` is its length of values, under the lock.
        let handle = unsafe {
            (self.fns().instance_new)(class, raw.as_ptr(), raw.len(), std::ptr::null_mut())
        };
        drop(fields);
        self.handle(tag::INSTANCE, handle)
    }

    /// An array of `elements`.
    pub fn array(&self, elements: Vec<BamlValue<'_>>) -> Result<BamlValue<'_>, SdkError> {
        let raw: Vec<RawValue> = elements.iter().map(BamlValue::raw).collect();
        // SAFETY: as above.
        let handle = unsafe { (self.fns().array_new)(raw.as_ptr(), raw.len()) };
        drop(elements);
        self.handle(tag::ARRAY, handle)
    }

    /// A map of `entries`, whose keys are strings this session made.
    pub fn map(
        &self,
        entries: Vec<(BamlValue<'_>, BamlValue<'_>)>,
    ) -> Result<BamlValue<'_>, SdkError> {
        let keys: Vec<RawValue> = entries.iter().map(|(key, _)| key.raw()).collect();
        let values: Vec<RawValue> = entries.iter().map(|(_, value)| value.raw()).collect();
        // SAFETY: as above.
        let handle = unsafe { (self.fns().map_new)(keys.as_ptr(), values.as_ptr(), keys.len()) };
        drop(entries);
        self.handle(tag::MAP, handle)
    }

    /// Variant `index` of the enum numbered `enumeration`.
    pub fn variant(&self, enumeration: u32, index: u32) -> Result<BamlValue<'_>, SdkError> {
        let mut raw = RawValue { tag: 0, payload: 0 };
        // SAFETY: as above.
        if unsafe { (self.fns().variant)(enumeration, index, &raw mut raw) } != 0 {
            return Err(self.refused());
        }
        Ok(self.owned(raw))
    }

    /// Give the library a turn, and deal with what it reported.
    fn turn(&self) {
        // SAFETY: under the lock.
        let next = unsafe { (self.fns().turn)() };
        let unhandled = std::mem::take(
            &mut self
                .library
                .inbox
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .unhandled,
        );
        if !unhandled.is_empty() {
            let hook = self
                .library
                .on_unhandled
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for raw in unhandled {
                let value = self.owned(raw);
                hook(&value.describe());
            }
        }
        if let Ok(nanos) = u64::try_from(next) {
            let timer = &self.library.timer;
            *timer
                .deadline
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                Some(Instant::now() + Duration::from_nanos(nanos));
            timer.changed.notify_one();
        }
    }
}

/// Read text out of the library with `read(out, capacity)`, which returns
/// the full length, or a negative number when there is none.
///
/// # Safety
///
/// `read` writes at most `capacity` bytes to `out`.
unsafe fn text_from(read: impl Fn(*mut u8, usize) -> i64) -> Option<String> {
    let mut buffer = vec![0_u8; 64];
    let length = usize::try_from(read(buffer.as_mut_ptr(), buffer.len())).ok()?;
    if length > buffer.len() {
        buffer.resize(length, 0);
        read(buffer.as_mut_ptr(), buffer.len());
    }
    buffer.truncate(length);
    String::from_utf8(buffer).ok()
}

/// A value inside a [`Session`]: a scalar, or an object it holds a handle
/// to, which is freed when the value is dropped.
pub struct BamlValue<'s> {
    raw: RawValue,
    session: &'s Session,
}

impl Drop for BamlValue<'_> {
    fn drop(&mut self) {
        if self.raw.tag & tag::HANDLE != 0 {
            // SAFETY: a handle this value owns, under the session's lock.
            unsafe { (self.session.fns().handle_free)(self.raw.payload) };
        }
    }
}

fn decode(what: impl Into<String>) -> SdkError {
    SdkError::Decode(what.into())
}

impl<'s> BamlValue<'s> {
    pub fn raw(&self) -> RawValue {
        self.raw
    }

    /// Its type, without the handle bit.
    pub fn kind(&self) -> u64 {
        self.raw.tag & tag::TYPE
    }

    fn handle(&self) -> Result<u64, SdkError> {
        if self.raw.tag & tag::HANDLE == 0 {
            Err(decode(format!(
                "a scalar of tag {} where an object was expected",
                self.kind()
            )))
        } else {
            Ok(self.raw.payload)
        }
    }

    fn fns(&self) -> &'s Fns {
        self.session.fns()
    }

    pub fn is_null(&self) -> bool {
        self.raw.tag == tag::NULL
    }

    pub fn int(&self) -> Result<i64, SdkError> {
        (self.raw.tag == tag::INT)
            .then(|| self.raw.payload.cast_signed())
            .ok_or_else(|| decode(format!("tag {} where an int was expected", self.kind())))
    }

    pub fn float(&self) -> Result<f64, SdkError> {
        (self.raw.tag == tag::FLOAT)
            .then(|| f64::from_bits(self.raw.payload))
            .ok_or_else(|| decode(format!("tag {} where a float was expected", self.kind())))
    }

    pub fn bool(&self) -> Result<bool, SdkError> {
        (self.raw.tag == tag::BOOL)
            .then_some(self.raw.payload & 0xFF != 0)
            .ok_or_else(|| decode(format!("tag {} where a bool was expected", self.kind())))
    }

    pub fn is_string(&self) -> bool {
        matches!(self.kind(), tag::STRING | tag::STRING_SLICE) && self.raw.tag & tag::HANDLE != 0
    }

    pub fn string(&self) -> Result<String, SdkError> {
        let handle = self.handle()?;
        // SAFETY: under the lock; a buffer of its length.
        unsafe { text_from(|out, capacity| (self.fns().string_copy)(handle, out, capacity)) }
            .ok_or_else(|| decode("not a string"))
    }

    /// The value as an uncaught throw of it is written.
    pub fn describe(&self) -> String {
        let Ok(handle) = self.handle() else {
            return format!("{:?}", self.raw);
        };
        // SAFETY: as above.
        unsafe { text_from(|out, capacity| (self.fns().describe)(handle, out, capacity)) }
            .unwrap_or_default()
    }

    /// The number of its class or enum in the library's interface.
    pub fn class_of(&self) -> Option<u32> {
        let handle = self.handle().ok()?;
        // SAFETY: under the lock.
        u32::try_from(unsafe { (self.fns().class_of)(handle) }).ok()
    }

    /// Check that it is an instance of the class numbered `class`.
    pub fn expect_class(&self, class: u32) -> Result<(), SdkError> {
        if self.class_of() == Some(class) {
            Ok(())
        } else {
            Err(decode(format!("expected an instance of class {class}")))
        }
    }

    /// Field `index`, in declaration order.
    pub fn field(&self, index: u32) -> Result<BamlValue<'s>, SdkError> {
        let mut raw = RawValue { tag: 0, payload: 0 };
        // SAFETY: under the lock.
        if unsafe { (self.fns().field_get)(self.handle()?, index, &raw mut raw) } != 0 {
            return Err(self.session.refused());
        }
        Ok(self.session.owned(raw))
    }

    /// Its elements, when it is an array.
    pub fn elements(
        &self,
    ) -> Result<impl Iterator<Item = Result<BamlValue<'s>, SdkError>> + '_, SdkError> {
        let handle = self.handle()?;
        // SAFETY: under the lock.
        let length = u64::try_from(unsafe { (self.fns().array_len)(handle) })
            .map_err(|_| self.session.refused())?;
        Ok((0..length).map(move |index| {
            let mut raw = RawValue { tag: 0, payload: 0 };
            // SAFETY: under the lock.
            if unsafe { (self.fns().array_get)(handle, index, &raw mut raw) } != 0 {
                return Err(self.session.refused());
            }
            Ok(self.session.owned(raw))
        }))
    }

    /// Its entries in insertion order, when it is a map.
    pub fn entries(&self) -> Result<Vec<(BamlValue<'s>, BamlValue<'s>)>, SdkError> {
        let handle = self.handle()?;
        let mut entries = Vec::new();
        let mut cursor = 0;
        loop {
            let mut key = RawValue { tag: 0, payload: 0 };
            let mut value = RawValue { tag: 0, payload: 0 };
            // SAFETY: under the lock.
            let next =
                unsafe { (self.fns().map_next)(handle, cursor, &raw mut key, &raw mut value) };
            match next {
                -1 => return Ok(entries),
                next if next < 0 => return Err(self.session.refused()),
                next => {
                    entries.push((self.session.owned(key), self.session.owned(value)));
                    cursor = next.cast_unsigned();
                }
            }
        }
    }

    /// Its position in its enum, when it is an enum value.
    pub fn variant_index(&self) -> Result<u32, SdkError> {
        // SAFETY: under the lock.
        u32::try_from(unsafe { (self.fns().variant_index)(self.handle()?) })
            .map_err(|_| self.session.refused())
    }
}

/// Copying a Rust value into BAML memory.
pub trait ToBaml {
    fn to_baml<'s>(&self, session: &'s Session) -> Result<BamlValue<'s>, SdkError>;
}

/// Copying a value out of BAML memory into a Rust value.
pub trait FromBaml: Sized {
    fn from_baml(value: BamlValue<'_>) -> Result<Self, SdkError>;
}

impl ToBaml for i64 {
    fn to_baml<'s>(&self, session: &'s Session) -> Result<BamlValue<'s>, SdkError> {
        Ok(session.int(*self))
    }
}

impl FromBaml for i64 {
    fn from_baml(value: BamlValue<'_>) -> Result<Self, SdkError> {
        value.int()
    }
}

impl ToBaml for f64 {
    fn to_baml<'s>(&self, session: &'s Session) -> Result<BamlValue<'s>, SdkError> {
        Ok(session.float(*self))
    }
}

impl FromBaml for f64 {
    fn from_baml(value: BamlValue<'_>) -> Result<Self, SdkError> {
        value.float()
    }
}

impl ToBaml for bool {
    fn to_baml<'s>(&self, session: &'s Session) -> Result<BamlValue<'s>, SdkError> {
        Ok(session.bool(*self))
    }
}

impl FromBaml for bool {
    fn from_baml(value: BamlValue<'_>) -> Result<Self, SdkError> {
        value.bool()
    }
}

impl ToBaml for String {
    fn to_baml<'s>(&self, session: &'s Session) -> Result<BamlValue<'s>, SdkError> {
        session.string(self)
    }
}

impl ToBaml for str {
    fn to_baml<'s>(&self, session: &'s Session) -> Result<BamlValue<'s>, SdkError> {
        session.string(self)
    }
}

impl FromBaml for String {
    fn from_baml(value: BamlValue<'_>) -> Result<Self, SdkError> {
        value.string()
    }
}

impl ToBaml for () {
    fn to_baml<'s>(&self, session: &'s Session) -> Result<BamlValue<'s>, SdkError> {
        Ok(session.null())
    }
}

impl FromBaml for () {
    fn from_baml(value: BamlValue<'_>) -> Result<Self, SdkError> {
        if value.is_null() {
            Ok(())
        } else {
            Err(decode("a value where null was expected"))
        }
    }
}

impl<T: ToBaml> ToBaml for Option<T> {
    fn to_baml<'s>(&self, session: &'s Session) -> Result<BamlValue<'s>, SdkError> {
        match self {
            Some(value) => value.to_baml(session),
            None => Ok(session.null()),
        }
    }
}

impl<T: FromBaml> FromBaml for Option<T> {
    fn from_baml(value: BamlValue<'_>) -> Result<Self, SdkError> {
        if value.is_null() {
            Ok(None)
        } else {
            T::from_baml(value).map(Some)
        }
    }
}

impl<T: ToBaml> ToBaml for Vec<T> {
    fn to_baml<'s>(&self, session: &'s Session) -> Result<BamlValue<'s>, SdkError> {
        let elements = self
            .iter()
            .map(|element| element.to_baml(session))
            .collect::<Result<Vec<_>, _>>()?;
        session.array(elements)
    }
}

impl<T: FromBaml> FromBaml for Vec<T> {
    fn from_baml(value: BamlValue<'_>) -> Result<Self, SdkError> {
        value
            .elements()?
            .map(|element| T::from_baml(element?))
            .collect()
    }
}

impl<V: ToBaml> ToBaml for Map<String, V> {
    fn to_baml<'s>(&self, session: &'s Session) -> Result<BamlValue<'s>, SdkError> {
        let entries = self
            .iter()
            .map(|(key, value)| Ok((session.string(key)?, value.to_baml(session)?)))
            .collect::<Result<Vec<_>, SdkError>>()?;
        session.map(entries)
    }
}

impl<V: FromBaml> FromBaml for Map<String, V> {
    fn from_baml(value: BamlValue<'_>) -> Result<Self, SdkError> {
        value
            .entries()?
            .into_iter()
            .map(|(key, value)| Ok((key.string()?, V::from_baml(value)?)))
            .collect()
    }
}

/// What a function that declares no `throws` cannot throw: anything it
/// throws arrives as [`Error::Runtime`].
impl FromBaml for std::convert::Infallible {
    fn from_baml(_value: BamlValue<'_>) -> Result<Self, SdkError> {
        Err(decode("a function that throws nothing threw"))
    }
}

/// A call in flight: the future of its result. Dropping it does not
/// cancel the BAML call; its result is freed when it arrives.
pub struct Call<T, E> {
    library: &'static Library,
    id: u64,
    failed: Option<SdkError>,
    done: bool,
    result: PhantomData<fn() -> (T, E)>,
}

impl<T: FromBaml, E: FromBaml> Call<T, E> {
    /// Copy what the call ended with into Rust values.
    fn finish(&self, status: u32, raw: RawValue) -> Result<T, Error<E>> {
        let session = self.library.session();
        let value = session.owned(raw);
        match status {
            STATUS_RETURNED => T::from_baml(value).map_err(Error::Sdk),
            STATUS_THREW => {
                let message = value.describe();
                let class_name = (value.kind() == tag::INSTANCE).then(|| {
                    message
                        .split([' ', '{'])
                        .next()
                        .unwrap_or_default()
                        .to_owned()
                });
                match E::from_baml(value) {
                    Ok(thrown) => Err(Error::Thrown {
                        value: Box::new(thrown),
                    }),
                    Err(_) => Err(Error::Runtime {
                        class_name,
                        message,
                    }),
                }
            }
            STATUS_STUCK => Err(Error::Stuck),
            STATUS_PANICKED => Err(Error::Panic {
                message: value.describe(),
            }),
            other => Err(Error::Sdk(decode(format!(
                "a call ended with status {other}"
            )))),
        }
    }
}

impl<T: FromBaml, E: FromBaml> Future for Call<T, E> {
    type Output = Result<T, Error<E>>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(error) = self.failed.take() {
            self.done = true;
            return Poll::Ready(Err(Error::Sdk(error)));
        }
        let library = self.library;
        loop {
            let ended = {
                let mut inbox = library
                    .inbox
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let ended = inbox.ended.remove(&self.id);
                if ended.is_none() {
                    // Registered before the turn is tried, so no wake is lost.
                    inbox.waiting.insert(self.id, context.waker().clone());
                }
                ended
            };
            if let Some((status, raw)) = ended {
                self.done = true;
                return Poll::Ready(self.finish(status, raw));
            }
            if library.need_turn.swap(false, Ordering::AcqRel) {
                library.session().turn();
                continue;
            }
            return Poll::Pending;
        }
    }
}

impl<T, E> Drop for Call<T, E> {
    fn drop(&mut self) {
        if self.done || self.id == 0 {
            return;
        }
        let mut inbox = self
            .library
            .inbox
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inbox.waiting.remove(&self.id);
        match inbox.ended.remove(&self.id) {
            Some((_, raw)) if raw.tag & tag::HANDLE != 0 => inbox.frees.push(raw.payload),
            Some(_) => {}
            None => {
                inbox.abandoned.insert(self.id);
            }
        }
        drop(inbox);
        // Another call may be waiting for the turn this one would have run.
        self.library.wake_all();
    }
}
