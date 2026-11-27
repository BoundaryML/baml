//! Calling a BAML program compiled to a native library from a host
//! program.
//!
//! `native_pack --lib` makes a shared library that exports the C functions
//! of `baml_native_rt/src/sched/host.rs` (`baml_native_rt/design/bridges.md`).
//! This crate is in two layers:
//!
//! - **The core**, here: loading the library and checking its interface
//!   hash ([`Library::load`]), the lock every call into it takes
//!   ([`Session`]), starting a call from built values ([`Session::start`]),
//!   running a turn and taking what ended ([`Session::turn`]), reading and
//!   building values ([`BamlValue`], [`ToBaml`], [`FromBaml`]), and the
//!   interface as a typed tree ([`Library::interface`]). No executor and no
//!   thread of its own: a Python or Node module drives it from its own
//!   event loop.
//! - **The futures layer**, [`futures`]: [`futures::Call`], a Rust future
//!   per call, a timer thread, and who is woken. It is built on the core's
//!   public functions alone.
//!
//! User code never holds a handle: a [`BamlValue`] lives inside a
//! [`Session`] and frees its handle when dropped; [`Detached`] carries one
//! past a session, and frees it at the next one if dropped.

#![expect(
    unsafe_code,
    clippy::missing_transmute_annotations,
    reason = "a C interface found by name in a library loaded at run time, each function \
              transmuted to the type of the field it is kept in"
)]

pub mod futures;
mod interface;
mod values;

use std::{
    cell::UnsafeCell,
    ffi::{CString, c_char, c_int, c_void},
    marker::PhantomData,
    sync::{
        Mutex, MutexGuard, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub use futures::Error;
/// The map type generated bindings use, as today's Rust SDK does.
pub use indexmap::IndexMap as Map;
pub use interface::{Field, Function, Interface, Param, Type, TypeDef, TypeDefKind};
pub use values::{BamlValue, Detached, FromBaml, ToBaml};

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

/// How a call ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// It returned its value.
    Returned,
    /// It threw the value.
    Threw,
    /// It can never finish: nothing can run, no timer is set and no I/O
    /// can complete. The value is `null`.
    Stuck,
    /// It panicked (`baml.panics`, `baml.panics.Exit` included) with the
    /// value.
    Panicked,
}

/// What a turn reports.
pub enum Event<'s> {
    /// The call `call` ended.
    Ended {
        call: u64,
        outcome: Outcome,
        value: BamlValue<'s>,
    },
    /// A task the call `call` spawned failed, and nothing can await it
    /// any more.
    Unhandled { call: u64, error: BamlValue<'s> },
}

/// One exported function, as the library describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Export {
    pub name: String,
    pub arity: u32,
    /// The signature as text: `add(a: int, b: int) -> int`.
    pub signature: String,
    pub hash: u64,
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
pub(crate) struct Fns {
    init: unsafe extern "C" fn(*const Callbacks) -> i32,
    find: unsafe extern "C" fn(*const u8, usize) -> i64,
    export_count: unsafe extern "C" fn() -> u64,
    export_info: unsafe extern "C" fn(u64, *mut u32, *mut u64, *mut u8, usize) -> i64,
    interface_hash: unsafe extern "C" fn() -> u64,
    interface_json: unsafe extern "C" fn(*mut u8, usize) -> i64,
    call_start: unsafe extern "C" fn(u64, *const RawValue, usize) -> u64,
    turn: unsafe extern "C" fn() -> i64,
    last_error: unsafe extern "C" fn(*mut u8, usize) -> i64,
    pub(crate) handle_free: unsafe extern "C" fn(u64),
    pub(crate) describe: unsafe extern "C" fn(u64, *mut u8, usize) -> i64,
    collect: unsafe extern "C" fn(),
    handle_count: unsafe extern "C" fn() -> u64,
    string_new: unsafe extern "C" fn(*const u8, usize) -> u64,
    pub(crate) string_copy: unsafe extern "C" fn(u64, *mut u8, usize) -> i64,
    pub(crate) class_of: unsafe extern "C" fn(u64) -> i64,
    pub(crate) field_get: unsafe extern "C" fn(u64, u32, *mut RawValue) -> i32,
    pub(crate) array_len: unsafe extern "C" fn(u64) -> i64,
    pub(crate) array_get: unsafe extern "C" fn(u64, u64, *mut RawValue) -> i32,
    pub(crate) map_next: unsafe extern "C" fn(u64, u64, *mut RawValue, *mut RawValue) -> i64,
    pub(crate) variant_index: unsafe extern "C" fn(u64) -> i64,
    variant: unsafe extern "C" fn(u32, u32, *mut RawValue) -> i32,
    instance_new: unsafe extern "C" fn(u32, *const RawValue, usize, *mut i32) -> u64,
    array_new: unsafe extern "C" fn(*const RawValue, usize) -> u64,
    map_new: unsafe extern "C" fn(*const RawValue, *const RawValue, usize) -> u64,
}

/// What the library's callbacks report during a turn, before it is handed
/// over.
#[derive(Clone, Copy)]
enum RawEvent {
    Ended(u64, u32, RawValue),
    Unhandled(u64, RawValue),
}

type WakeFn = Box<dyn Fn() + Send + Sync>;

/// A loaded library. One per process.
pub struct Library {
    fns: Fns,
    path: String,
    /// Every call into the library holds it: one thread at a time.
    lock: Mutex<()>,
    /// What the callbacks reported in the turn running now; the vector
    /// keeps its room from turn to turn. Only touched while `lock` is held:
    /// by a session, and by the callbacks, which run inside a turn.
    events: UnsafeCell<Vec<RawEvent>>,
    /// Handles of [`Detached`] values dropped outside a session, freed at
    /// the next one, and whether there are any.
    frees: Mutex<Vec<u64>>,
    has_frees: AtomicBool,
    /// What the library calls, from any thread, when it wants a turn.
    wake: WakeFn,
    exports: Vec<Export>,
    interface_json: String,
    interface: Interface,
}

// SAFETY: `events`, the one field that is not `Sync` by itself, is only
// touched while `lock` is held.
unsafe impl Sync for Library {}

static LIBRARY: OnceLock<Library> = OnceLock::new();
/// Held while a library loads, so that two first loads do not race.
static LOADING: Mutex<()> = Mutex::new(());

/// Keep what a callback reported for the turn's session.
///
/// # Safety
///
/// Called inside a turn, which runs while a session holds the library.
unsafe fn report(event: RawEvent) {
    if let Some(library) = LIBRARY.get() {
        // SAFETY: the caller's contract: the session's lock is held.
        unsafe { (*library.events.get()).push(event) };
    }
}

unsafe extern "C" fn on_done(_ctx: *mut c_void, call: u64, status: u32, tag: u64, payload: u64) {
    // SAFETY: the library calls it only inside a turn.
    unsafe { report(RawEvent::Ended(call, status, RawValue { tag, payload })) };
}

unsafe extern "C" fn on_unhandled(_ctx: *mut c_void, call: u64, tag: u64, payload: u64) {
    // SAFETY: as above.
    unsafe { report(RawEvent::Unhandled(call, RawValue { tag, payload })) };
}

unsafe extern "C" fn wake(_ctx: *mut c_void) {
    if let Some(library) = LIBRARY.get() {
        (library.wake)();
    }
}

impl Library {
    /// Open the library at `path`, refuse it when its interface hash is not
    /// `expected_hash` (the bindings'), and build its worker. `wake` is
    /// called from any thread when the library wants a turn: the host then
    /// takes a [`Session`] and calls [`Session::turn`] on its own thread.
    ///
    /// One library per process. Loading the same path again returns it,
    /// after checking the hash (the first `wake` stays); another path is an
    /// error.
    pub fn load(
        path: &str,
        expected_hash: u64,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<&'static Self, SdkError> {
        let _loading = LOADING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(library) = LIBRARY.get() {
            if library.path != path {
                return Err(SdkError::Load(format!(
                    "a library is loaded already, from {}: one per process",
                    library.path
                )));
            }
            return library.check_hash(expected_hash).map(|()| library);
        }
        let fns = open(path)?;
        // SAFETY: reads the library's table of exports, which needs no init.
        let actual = unsafe { (fns.interface_hash)() };
        if actual != expected_hash {
            return Err(hash_mismatch(actual, expected_hash));
        }
        let exports = exports_of(&fns);
        // SAFETY: as above; a buffer of its length.
        let interface_json =
            unsafe { text_from(|out, capacity| (fns.interface_json)(out, capacity)) }
                .unwrap_or_default();
        let interface = Interface::from_json(&interface_json)?;
        let library = LIBRARY.get_or_init(|| Self {
            fns,
            path: path.to_owned(),
            lock: Mutex::new(()),
            events: UnsafeCell::new(Vec::new()),
            frees: Mutex::new(Vec::new()),
            has_frees: AtomicBool::new(false),
            wake: Box::new(wake),
            exports,
            interface_json,
            interface,
        });
        let callbacks = Callbacks {
            size: u32::try_from(size_of::<Callbacks>()).expect("a small struct"),
            ctx: std::ptr::null_mut(),
            on_done,
            on_unhandled,
            wake: self::wake,
        };
        let _guard = library.lock();
        // SAFETY: once per process, before any other call into the library.
        let status = unsafe { (library.fns.init)(&raw const callbacks) };
        if status != 0 {
            return Err(SdkError::Load(format!("init failed with {status}")));
        }
        Ok(library)
    }

    /// The library loaded in this process, if one is.
    pub fn loaded() -> Option<&'static Self> {
        LIBRARY.get()
    }

    fn check_hash(&self, expected: u64) -> Result<(), SdkError> {
        // SAFETY: reads the library's table.
        let actual = unsafe { (self.fns.interface_hash)() };
        if actual == expected {
            Ok(())
        } else {
            Err(hash_mismatch(actual, expected))
        }
    }

    /// Its exported functions, numbered by their place here.
    pub fn exports(&self) -> &[Export] {
        &self.exports
    }

    /// Its interface as JSON, as `native_pack --lib --describe` prints it:
    /// every export with its parameters, and every class and enum with its
    /// fields and variants, numbered.
    pub fn interface_json(&self) -> &str {
        &self.interface_json
    }

    /// Its interface as a typed tree, parsed once at load: what bindings
    /// build their plans from.
    pub fn interface(&self) -> &Interface {
        &self.interface
    }

    fn lock(&self) -> MutexGuard<'_, ()> {
        self.lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The library, held by this thread: every call into it goes through a
    /// session. Handles of [`Detached`] values dropped since the last one
    /// are freed now.
    pub fn session(&'static self) -> Session {
        let guard = self.lock();
        if self.has_frees.swap(false, Ordering::AcqRel) {
            let frees = std::mem::take(
                &mut *self
                    .frees
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
            );
            for handle in frees {
                // SAFETY: a handle this crate owned, under the lock.
                unsafe { (self.fns.handle_free)(handle) };
            }
        }
        Session {
            library: self,
            _guard: guard,
            not_send: PhantomData,
        }
    }

    /// Free `handle` at the next session.
    pub(crate) fn free_later(&self, handle: u64) {
        self.frees
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(handle);
        self.has_frees.store(true, Ordering::Release);
    }

    /// The number of an export, by name.
    pub fn find(&self, name: &str) -> Option<u32> {
        // SAFETY: reads the table of exports.
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

fn hash_mismatch(actual: u64, expected: u64) -> SdkError {
    SdkError::Load(format!(
        "the library's interface hash is {actual:016x}, the bindings' {expected:016x}: \
         the bindings were generated for another version of the program"
    ))
}

fn exports_of(fns: &Fns) -> Vec<Export> {
    // SAFETY: reads the table of exports.
    let count = unsafe { (fns.export_count)() };
    (0..count)
        .filter_map(|index| {
            let (mut arity, mut hash) = (0, 0);
            // SAFETY: as above; buffers of their lengths.
            let signature = unsafe {
                text_from(|out, capacity| {
                    (fns.export_info)(index, &raw mut arity, &raw mut hash, out, capacity)
                })
            }?;
            let name = signature.split('(').next().unwrap_or_default().to_owned();
            Some(Export {
                name,
                arity,
                signature,
                hash,
            })
        })
        .collect()
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
        unsafe { std::mem::transmute(symbol_of(path, "baml_lib_abi_version")?) };
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
            unsafe { std::mem::transmute(symbol_of(path, $name)?) }
        };
    }
    Ok(Fns {
        init: function!("baml_lib_init"),
        find: function!("baml_lib_find"),
        export_count: function!("baml_lib_export_count"),
        export_info: function!("baml_lib_export_info"),
        interface_hash: function!("baml_lib_interface_hash"),
        interface_json: function!("baml_lib_interface_json"),
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

/// The library held by one thread: what starts calls, runs turns, and
/// reads and builds values. A [`BamlValue`] cannot outlive it. Neither can
/// cross to another thread: the library is entered by one thread at a
/// time.
pub struct Session {
    library: &'static Library,
    _guard: MutexGuard<'static, ()>,
    not_send: PhantomData<*const ()>,
}

/// `Session` is neither `Send` nor `Sync`, and so a `BamlValue`, which
/// holds a `&Session`, is neither either.
const _: () = {
    trait AmbiguousIfImplemented<A> {
        fn some_item() {}
    }
    impl<T: ?Sized> AmbiguousIfImplemented<()> for T {}
    struct Send_;
    struct Sync_;
    impl<T: ?Sized + Send> AmbiguousIfImplemented<Send_> for T {}
    impl<T: ?Sized + Sync> AmbiguousIfImplemented<Sync_> for T {}
    // Each line fails to compile, as ambiguous, if the type is Send or
    // Sync.
    let _ = <Session as AmbiguousIfImplemented<_>>::some_item;
    let _ = <BamlValue<'static> as AmbiguousIfImplemented<_>>::some_item;
};

impl Session {
    pub(crate) fn fns(&self) -> &Fns {
        &self.library.fns
    }

    /// The library it holds.
    pub fn library(&self) -> &'static Library {
        self.library
    }

    /// Why the library refused the last thing it refused.
    pub fn last_error(&self) -> String {
        // SAFETY: under the lock; a buffer of its length.
        unsafe { text_from(|out, capacity| (self.fns().last_error)(out, capacity)) }
            .unwrap_or_default()
    }

    pub(crate) fn refused(&self) -> SdkError {
        SdkError::Refused(self.last_error())
    }

    pub(crate) fn owned(&self, raw: RawValue) -> BamlValue<'_> {
        BamlValue::new(raw, self)
    }

    /// Bring a value carried past its session back into this one.
    pub fn attach(&self, value: Detached) -> BamlValue<'_> {
        self.owned(value.into_raw())
    }

    /// Start export `export` with `args`, each built in this session.
    /// Returns the call's id. The arguments stay the caller's, but an
    /// array or a map is taken: it can be passed once.
    pub fn start(&self, export: u32, args: &[BamlValue<'_>]) -> Result<u64, SdkError> {
        // Up to eight arguments are passed from the stack.
        let mut few = [RawValue { tag: 0, payload: 0 }; 8];
        let many: Vec<RawValue>;
        let raw: &[RawValue] = if args.len() <= few.len() {
            for (place, arg) in few.iter_mut().zip(args) {
                *place = arg.raw();
            }
            &few[..args.len()]
        } else {
            many = args.iter().map(BamlValue::raw).collect();
            &many
        };
        // SAFETY: `raw` is its length of values, under the lock.
        let id = unsafe { (self.fns().call_start)(u64::from(export), raw.as_ptr(), raw.len()) };
        if id == 0 { Err(self.refused()) } else { Ok(id) }
    }

    /// Give the library a turn: run BAML until every task has ended or
    /// waits. Then hand `each` what ended, in order, while this session
    /// still holds the library, so the values can be copied on the spot.
    /// Returns how long until the next BAML timer fires, or `None` when
    /// none is set: the host turns again then, or when `wake` comes.
    pub fn turn(&self, mut each: impl FnMut(Event<'_>)) -> Option<Duration> {
        // SAFETY: under the lock.
        let next = unsafe { (self.fns().turn)() };
        // SAFETY: this session holds the lock; the turn is over, so no
        // callback runs while it is taken.
        let mut events = std::mem::take(unsafe { &mut *self.library.events.get() });
        for event in events.drain(..) {
            each(match event {
                RawEvent::Ended(call, status, raw) => Event::Ended {
                    call,
                    outcome: match status {
                        0 => Outcome::Returned,
                        1 => Outcome::Threw,
                        2 => Outcome::Stuck,
                        _ => Outcome::Panicked,
                    },
                    value: self.owned(raw),
                },
                RawEvent::Unhandled(call, raw) => Event::Unhandled {
                    call,
                    error: self.owned(raw),
                },
            });
        }
        // The emptied vector keeps its room for the next turn. Had `each`
        // turned again through this session, what that turn reported was
        // handed over by it already.
        // SAFETY: as above.
        unsafe { *self.library.events.get() = events };
        u64::try_from(next).ok().map(Duration::from_nanos)
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
}

/// Read text out of the library with `read(out, capacity)`, which returns
/// the full length, or a negative number when there is none.
///
/// # Safety
///
/// `read` writes at most `capacity` bytes to `out`.
pub(crate) unsafe fn text_from(mut read: impl FnMut(*mut u8, usize) -> i64) -> Option<String> {
    let mut buffer = vec![0_u8; 64];
    let length = usize::try_from(read(buffer.as_mut_ptr(), buffer.len())).ok()?;
    if length > buffer.len() {
        buffer.resize(length, 0);
        read(buffer.as_mut_ptr(), buffer.len());
    }
    buffer.truncate(length);
    String::from_utf8(buffer).ok()
}
