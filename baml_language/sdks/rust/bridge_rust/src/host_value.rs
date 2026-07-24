//! Host-value registry and BAML→host callable dispatch.
//!
//! When Rust passes a closure as an argument to a BAML function, the
//! generated binding erases it through [`HostCallback`] and registers it
//! here, sending `InboundValue::Handle{key, HOST_VALUE_CALLABLE}` on the
//! wire. The engine binds the handle to an `Object::HostClosure`; when
//! BAML invokes it, the engine fires the process-global dispatch callback
//! registered via the C ABI, which lands in [`host_dispatch_callback`]:
//! decode the `BamlToHostCall` args, run the user closure on the bridge's
//! own dispatch runtime, and complete the call with an `InboundValue`
//! payload via `complete_host_call`.
//!
//! A fallible closure's error takes one of two paths, matching the other
//! bridges:
//! - the declared BAML throws class (`E: BamlValue`) encodes as that real
//!   class — a BAML `catch (e: MyError)` matches it structurally;
//! - an arbitrary host error (`E: std::error::Error`) is registered here
//!   as an opaque entry and rides as a `baml.errors.HostCallable`
//!   instance whose `_handle` field references it. If BAML re-throws it
//!   back out, it decodes as the normal throws member [`HostCallable`],
//!   whose `_handle` resolves through the registry back to the original
//!   value ([`HostCallable::downcast_ref`]) — the same-host round trip.
//!
//! The engine releases registry entries through the release callback at
//! its safepoints; bridge-layer faults (unknown key, malformed args, a
//! panicking closure) complete with the empty-error payload, which the
//! engine surfaces as an SDK panic rather than a catchable throw.

use std::{
    any::Any,
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use prost::Message as _;

use crate::{
    BamlValue, capi,
    loader::log,
    wire::{self, inbound_value::Value as In},
};

/// One declared parameter of a BAML callable type, as the generated
/// binding describes it to the dispatcher: required params arrive
/// positionally, optional params by name (absent when BAML omits them).
#[derive(Debug, Clone, Copy)]
pub struct HostParam {
    pub name: &'static str,
    pub optional: bool,
}

/// How a host throw crosses back into the engine. Public only because it
/// appears in [`HostCallback::erase`]'s erased signature; never named by
/// generated or user code.
#[doc(hidden)]
pub enum HostThrow {
    /// The closure's error type is the declared BAML throws class —
    /// encoded as that real class so a typed BAML `catch` matches.
    Typed(wire::InboundValue),
    /// An arbitrary host error: registered as an opaque entry and sent as
    /// a `baml.errors.HostCallable` instance referencing it.
    Opaque {
        class_name: String,
        message: String,
        original: Arc<dyn Any + Send + Sync>,
    },
}

#[doc(hidden)]
pub type DispatchFuture =
    Pin<Box<dyn Future<Output = Result<wire::InboundValue, HostThrow>> + Send>>;

/// A registered host closure: decode the incoming args, run the user
/// closure, produce the encoded result (or throw). The `Err` arm is an
/// argument-decode failure — a bridge fault, not a user throw. A concrete
/// type (not a `dyn Fn` alias) so the registry can hold it as `dyn Any`
/// and downcast it back. Public only through [`HostCallback::erase`]'s
/// signature.
#[doc(hidden)]
pub struct ErasedCallable(
    Box<dyn Fn(Vec<wire::BamlToHostArg>) -> Result<DispatchFuture, String> + Send + Sync>,
);

struct Registry {
    next_key: AtomicU64,
    /// Every host value — a registered [`ErasedCallable`] or an opaque
    /// original error — is stored type-erased. Each key has exactly one
    /// use site (fixed at registration: a dispatch key is always a
    /// callable, a throw's `_handle` always an error), which downcasts to
    /// the type it expects, so the table needs no per-entry tag.
    table: Mutex<HashMap<u64, Arc<dyn Any + Send + Sync>>>,
}

static REGISTRY: LazyLock<Registry> = LazyLock::new(|| Registry {
    next_key: AtomicU64::new(1),
    table: Mutex::new(HashMap::new()),
});

/// The bridge-owned runtime that runs user closures. The engine's own
/// tokio lives inside the dylib (a separate tokio instance), so its
/// worker threads carry no context this crate's tokio can see — and user
/// closures must not block engine workers anyway.
static DISPATCH_RT: LazyLock<tokio::runtime::Runtime> = LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("baml-host-dispatch")
        .enable_all()
        .build()
        .unwrap_or_else(|e| unreachable!("building the host-dispatch runtime cannot fail: {e}"))
});

fn next_key() -> u64 {
    loop {
        let k = REGISTRY.next_key.fetch_add(1, Ordering::Relaxed);
        if k != 0 {
            return k;
        }
    }
}

/// Register a host value (callable or opaque error) and return its key.
/// The entry stays until the engine's release callback fires.
fn insert(value: Arc<dyn Any + Send + Sync>) -> u64 {
    let key = next_key();
    match REGISTRY.table.lock() {
        Ok(mut table) => {
            table.insert(key, value);
        }
        Err(e) => {
            // Poisoned: an earlier panic happened while holding the lock.
            // The entry is dropped; the engine-side handle will dangle and
            // surface as a bridge failure on dispatch — loud, not silent.
            log::warn(&format!(
                "host-value registry mutex poisoned while inserting key {key}: {e}"
            ));
        }
    }
    key
}

/// Look up a host value by key, if this process registered it and the
/// engine has not yet released it. The caller downcasts to the kind it
/// expects.
fn lookup(key: u64) -> Option<Arc<dyn Any + Send + Sync>> {
    match REGISTRY.table.lock() {
        Ok(table) => table.get(&key).map(Arc::clone),
        Err(e) => {
            log::warn(&format!(
                "host-value registry mutex poisoned during lookup of key {key}: {e}"
            ));
            None
        }
    }
}

/// Register `cb` (via its [`HostCallback`] erasure) and return the wire
/// value the generated binding sends for the callable argument.
pub fn callable_handle<Args, Ret, Marker>(
    cb: impl HostCallback<Args, Ret, Marker>,
    params: &'static [HostParam],
) -> wire::InboundValue {
    let key = insert(cb.erase(params));
    wire::InboundValue {
        value_type: None,
        value: Some(In::Handle(wire::BamlHandle {
            key,
            handle_type: wire::BamlHandleType::HostValueCallable as i32,
        })),
    }
}

/// Install the dispatch + release callbacks with the engine, once. Called
/// on every host→engine dispatch (alongside the result callback) so they
/// are always in place before the engine could hold a callable handle.
pub(crate) fn ensure_callbacks_registered(api: &'static capi::Api) {
    static REGISTERED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    REGISTERED.get_or_init(|| {
        // SAFETY: both callbacks match the engine's ABI and never unwind
        // (dispatch catches panics; release only takes a mutex).
        #[expect(unsafe_code)]
        unsafe {
            (api.register_host_dispatch_callback)(host_dispatch_callback);
            (api.register_host_release_callback)(host_release_callback);
        }
    });
}

/// Engine → bridge: a host-owned value was released at an engine
/// safepoint. Drop the registry entry (callable or opaque alike).
extern "C" fn host_release_callback(host_value_key: u64) {
    match REGISTRY.table.lock() {
        Ok(mut table) => {
            table.remove(&host_value_key);
        }
        Err(e) => {
            log::warn(&format!(
                "host-value registry mutex poisoned during release of key \
                 {host_value_key}: {e}; entry leaked"
            ));
        }
    }
}

/// Engine → bridge: BAML invoked a host callable. Copy the borrowed args,
/// resolve the closure, and hand execution to the dispatch runtime; the
/// engine's worker returns immediately and awaits the completion.
extern "C" fn host_dispatch_callback(
    host_value_key: u64,
    call_id: u32,
    args: *const u8,
    length: usize,
) {
    let bytes: Vec<u8> = if length == 0 || args.is_null() {
        Vec::new()
    } else {
        // SAFETY: the engine guarantees `args` is valid for `length` bytes
        // for the synchronous duration of this call.
        #[expect(unsafe_code)]
        unsafe { std::slice::from_raw_parts(args, length) }.to_vec()
    };

    // Resolve before spawning so a missing entry surfaces as a bridge
    // failure now — the engine holding a handle the bridge cannot find is
    // a bridge bug, not a user error.
    let Some(callable) = lookup(host_value_key).and_then(|v| v.downcast::<ErasedCallable>().ok())
    else {
        log::warn(&format!(
            "no host callable registered for key {host_value_key}"
        ));
        complete_bridge_failure(call_id);
        return;
    };

    let call = match wire::BamlToHostCall::decode(bytes.as_slice()) {
        Ok(call) => call,
        Err(e) => {
            log::warn(&format!("malformed BamlToHostCall payload: {e}"));
            complete_bridge_failure(call_id);
            return;
        }
    };

    let handle = DISPATCH_RT.handle().clone();
    DISPATCH_RT.spawn(async move {
        // Run the erased closure on the blocking pool: for the sync families it
        // runs the user closure body inline, so this both isolates a panic (as
        // a `JoinError` that still completes the call) and keeps a slow/blocking
        // sync closure off the 2-worker async pool. For the async families it
        // only builds the future, which is driven on the async pool below.
        let future = match handle.spawn_blocking(move || (callable.0)(call.args)).await {
            Ok(Ok(future)) => future,
            Ok(Err(message)) => {
                log::warn(&format!("host callable argument decode failed: {message}"));
                complete_bridge_failure(call_id);
                return;
            }
            Err(join_error) => {
                log::warn(&format!(
                    "host callable panicked during invocation: {join_error}"
                ));
                complete_bridge_failure(call_id);
                return;
            }
        };
        match handle.spawn(future).await {
            Ok(Ok(value)) => complete(call_id, 0, &value.encode_to_vec()),
            Ok(Err(throw)) => complete_throw(call_id, throw),
            Err(join_error) => {
                log::warn(&format!("host callable panicked: {join_error}"));
                complete_bridge_failure(call_id);
            }
        }
    });
}

fn complete(call_id: u32, is_error: i32, bytes: &[u8]) {
    let Ok(api) = capi::api() else {
        // Unreachable in practice: the engine just called us, so it is
        // loaded. Nothing to complete against if not.
        return;
    };
    // SAFETY: `bytes` is valid for its length for the duration of the
    // call; the engine copies before returning.
    #[expect(unsafe_code)]
    unsafe {
        (api.complete_host_call)(call_id, is_error, bytes.as_ptr().cast(), bytes.len());
    }
}

/// The empty error payload is the ABI's bridge-failure signal: the engine
/// surfaces it as an SDK panic, not a catchable throw.
fn complete_bridge_failure(call_id: u32) {
    complete(call_id, 1, &[]);
}

fn complete_throw(call_id: u32, throw: HostThrow) {
    match throw {
        HostThrow::Typed(value) => complete(call_id, 1, &value.encode_to_vec()),
        HostThrow::Opaque {
            class_name,
            message,
            original,
        } => {
            let value = encode_host_callable(&class_name, &message, original);
            complete(call_id, 1, &value.encode_to_vec());
        }
    }
}

/// Register `original` and build the `baml.errors.HostCallable` wire
/// instance referencing it. Always a fresh registration: the incoming
/// key's lifetime belongs to whatever engine object the value arrived on.
fn encode_host_callable(
    class_name: &str,
    message: &str,
    original: Arc<dyn Any + Send + Sync>,
) -> wire::InboundValue {
    host_callable_instance(class_name, message, insert(original))
}

/// Build a `baml.errors.HostCallable` instance around an already-registered
/// `_handle` key. `class_name` / `message` are metadata; `language` is
/// always `"rust"` (one bridge per process ⇒ Rust-origin) and `traceback`
/// always null (Rust has no interpreter trace) — both still sent
/// explicitly, since the engine's instance check requires every field.
fn host_callable_instance(class_name: &str, message: &str, key: u64) -> wire::InboundValue {
    fn string_field<'k>(key: &'k str, value: &str) -> (&'k str, wire::InboundValue) {
        (
            key,
            wire::InboundValue {
                value_type: None,
                value: Some(In::StringValue(value.to_string())),
            },
        )
    }
    let handle_field = (
        "_handle",
        wire::InboundValue {
            value_type: None,
            value: Some(In::Handle(wire::BamlHandle {
                key,
                handle_type: wire::BamlHandleType::HostValueOpaque as i32,
            })),
        },
    );
    crate::encode::class(
        "baml.errors.HostCallable",
        vec![],
        vec![
            string_field("message", message),
            string_field("class_name", class_name),
            string_field("language", "rust"),
            (
                "traceback",
                wire::InboundValue {
                    value_type: None,
                    value: None,
                },
            ),
            handle_field,
        ],
    )
}

/// Rust surface of the `baml.errors.HostCallable` class: an arbitrary
/// host error thrown inside a host callable, transported opaquely through
/// BAML. It appears as a normal `throws` member — generated contracts
/// naming `baml.errors.HostCallable` use it as (part of) their `E`, so it
/// arrives inside [`crate::Error::Thrown`] like any other throw.
///
/// `message` maps 1:1 onto the BAML class. BAML additionally holds an
/// opaque `_handle`; the Rust side rehydrates it into the retained
/// `original` host error. The type parameter `T` is that original's
/// static type. It defaults to `dyn Any + Send + Sync` — the erased form
/// every wire decode produces, since BAML erases the concrete type — so
/// plain `HostCallable` is the type generated contracts name. Recover the
/// concrete type either by decoding straight into `HostCallable<T>` (a
/// caller that knows the host-error type; [`from_baml`] validates the
/// rehydrated original really is a `T`) or by refining an erased value
/// after the fact with [`downcast`](HostCallable::downcast) /
/// [`downcast_ref`](HostCallable::downcast_ref).
/// [`original`](HostCallable::original) hands back the `Arc<T>` directly.
///
/// The wire class also carries `class_name` / `language` / `traceback`
/// metadata, none of it on the public API. `class_name` is a pure
/// function of the static type — `type_name::<T>()` — so it is never
/// stored: for a concrete `HostCallable<T>` it names `T`, and for the
/// erased default it is the honest `dyn Any + …`. `language` is always
/// `"rust"` (one bridge per process ⇒ Rust-origin) and `traceback` always
/// null (Rust has no interpreter trace); both are re-synthesized on
/// encode. The incoming wire `class_name` is ignored on decode — it is
/// unstable (`type_name` has no format guarantee) and redundant with the
/// TypeId-checked `original`.
///
/// The original is never absent: exactly one bridge exists per process
/// (bridge registration is first-call-wins), so every `_handle` was
/// minted by this registry, and the entry lives until the engine's
/// release fires — after the value can no longer arrive. A dead handle
/// at decode is therefore a lifetime bug and fails decode loudly rather
/// than degrading to metadata.
///
/// [`from_baml`]: crate::baml_value::internal::__BamlValuePrivate::from_baml
pub struct HostCallable<T: ?Sized + Send + Sync = dyn Any + Send + Sync> {
    /// The originating error's rendered message.
    pub message: String,
    /// The rehydrated original host error, typed `Arc<T>` (the default
    /// `dyn Any + Send + Sync` is the wire-erased form).
    original: Arc<T>,
}

impl<T: ?Sized + Send + Sync> HostCallable<T> {
    /// A shared clone of the retained original host error. The caller owns
    /// the `Arc<T>` and can hold or borrow it freely; on the erased
    /// default it is `Arc<dyn Any + Send + Sync>` (then `Arc::downcast`).
    /// Cloning is a refcount bump; the value is always present (see the
    /// type docs).
    pub fn original(&self) -> Arc<T> {
        Arc::clone(&self.original)
    }
}

impl HostCallable {
    /// Borrow the erased original as a `T`. `None` when the original is
    /// some other type — an ordinary dynamic query.
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.original.downcast_ref::<T>()
    }

    /// Refine the erased original to a concrete `T`, yielding a
    /// statically-typed `HostCallable<T>` whose `original()` is `Arc<T>`.
    /// `Err(self)` when the original is some other type — the erased value
    /// is handed back unchanged, no allocation lost.
    pub fn downcast<T: Any + Send + Sync>(self) -> Result<HostCallable<T>, Self> {
        match Arc::downcast::<T>(self.original) {
            Ok(original) => Ok(HostCallable {
                message: self.message,
                original,
            }),
            Err(original) => Err(HostCallable {
                message: self.message,
                original,
            }),
        }
    }
}

impl<T: ?Sized + Send + Sync> std::fmt::Debug for HostCallable<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostCallable")
            .field("message", &self.message)
            .field("class_name", &std::any::type_name::<T>())
            .field("original", &"<original>")
            .finish_non_exhaustive()
    }
}

impl<T: ?Sized + Send + Sync> std::fmt::Display for HostCallable<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", std::any::type_name::<T>(), self.message)
    }
}

/// Decode the shared shell of a `baml.errors.HostCallable` wire value: its
/// `message` and the original rehydrated behind `_handle`. The wire
/// `class_name` is ignored (see the type docs).
fn decode_host_callable_shell(
    v: wire::BamlOutboundValue,
) -> Result<(String, Arc<dyn Any + Send + Sync>), crate::DecodeError> {
    use crate::wire::baml_outbound_value::Value as Out;
    const FQN: &str = "baml.errors.HostCallable";
    let v = crate::decode::unwrap(v);
    let Some(Out::ClassValue(class)) = v.value else {
        return Err(crate::DecodeError::WrongType {
            expected: FQN,
            got: crate::baml_value::wire_variant_kind(&v),
        });
    };
    if class.name != FQN {
        return Err(crate::DecodeError::FqnMismatch {
            expected: FQN,
            got: class.name,
        });
    }
    let field = |name: &str| -> Option<wire::BamlOutboundValue> {
        class
            .fields
            .iter()
            .find(|f| f.key == name)
            .and_then(|f| f.value.clone())
            .map(crate::decode::unwrap)
    };
    let message = match field("message").map(|m| m.value) {
        Some(Some(Out::StringValue(s))) => s,
        _ => String::new(),
    };
    let handle_key = match field("_handle").map(|h| h.value) {
        Some(Some(Out::HandleValue(handle))) => handle.key,
        _ => {
            return Err(crate::DecodeError::MissingField {
                class: FQN,
                field: "_handle",
            });
        }
    };
    // One bridge per process + engine-managed entry lifetime ⇒ the handle
    // must resolve; a miss is a lifetime bug, never a normal outcome —
    // fail decode loudly.
    let original =
        lookup(handle_key).ok_or(crate::DecodeError::DeadHostHandle { key: handle_key })?;
    Ok((message, original))
}

// Deliberately NOT `std::error::Error`: that would make a closure erroring
// with `HostCallable<T>` ambiguous between the typed and opaque
// [`HostCallback`] families. As a `BamlValue` it takes the typed path —
// re-thrown as the real `baml.errors.HostCallable` class.
//
// The erased default and the concrete `HostCallable<T>` (below) are
// disjoint `BamlValue` impls: the concrete impl's `T` is implicitly
// `Sized`, which the unsized `dyn Any + Send + Sync` can never satisfy, so
// they never overlap.
impl crate::baml_value::internal::__BamlValuePrivate for HostCallable<dyn Any + Send + Sync> {
    fn to_baml(&self) -> wire::InboundValue {
        encode_host_callable(
            std::any::type_name::<dyn Any + Send + Sync>(),
            &self.message,
            Arc::clone(&self.original),
        )
    }

    fn from_baml(v: wire::BamlOutboundValue) -> Result<Self, crate::DecodeError> {
        let (message, original) = decode_host_callable_shell(v)?;
        Ok(HostCallable { message, original })
    }

    fn baml_ty() -> wire::BamlTy {
        crate::baml_value::internal::class_ty("baml.errors.HostCallable", vec![])
    }
}

// A caller that statically knows the host-error type decodes straight into
// `HostCallable<T>`: `from_baml` rehydrates the opaque original and
// validates it really is a `T`. A mismatch means the value is some *other*
// host error, which `decode::decode_result` folds into `Error::Runtime`
// (the same fallback as any non-declared error-arm value).
impl<T: Any + Send + Sync> crate::baml_value::internal::__BamlValuePrivate for HostCallable<T> {
    fn to_baml(&self) -> wire::InboundValue {
        // Bind as `Arc<T>` first; the unsize coercion to the erased
        // `Arc<dyn Any + …>` then lands at the call argument (it cannot ride
        // through `Arc::clone`'s `-> Self` against an expected erased type).
        let original = Arc::clone(&self.original);
        encode_host_callable(std::any::type_name::<T>(), &self.message, original)
    }

    fn from_baml(v: wire::BamlOutboundValue) -> Result<Self, crate::DecodeError> {
        let (message, original) = decode_host_callable_shell(v)?;
        match Arc::downcast::<T>(original) {
            Ok(original) => Ok(HostCallable { message, original }),
            Err(_) => Err(crate::DecodeError::HostCallableTypeMismatch {
                expected: std::any::type_name::<T>(),
            }),
        }
    }

    fn baml_ty() -> wire::BamlTy {
        crate::baml_value::internal::class_ty("baml.errors.HostCallable", vec![])
    }
}

/// Slot the incoming args against the declared params: required params
/// arrive positionally in declared order (empty `arg_name`), supplied
/// optionals by name; an omitted optional becomes a BAML `null`, which an
/// `Option` parameter decodes as `None`.
fn slot_args(
    params: &[HostParam],
    args: Vec<wire::BamlToHostArg>,
) -> Result<Vec<wire::BamlOutboundValue>, String> {
    let mut positional = Vec::new();
    let mut named: HashMap<String, wire::BamlOutboundValue> = HashMap::new();
    for arg in args {
        let value = arg.value.unwrap_or_default();
        if arg.is_optional_arg {
            named.insert(arg.arg_name, value);
        } else {
            positional.push(value);
        }
    }
    let mut positional = positional.into_iter();
    let mut slots = Vec::with_capacity(params.len());
    for param in params {
        if param.optional {
            slots.push(named.remove(param.name).unwrap_or_default());
        } else {
            slots.push(
                positional
                    .next()
                    .ok_or_else(|| format!("missing required argument `{}`", param.name))?,
            );
        }
    }
    Ok(slots)
}

/// A closure (sync or async, infallible or fallible) that BAML can invoke
/// through the bridge. `Args` is the tuple of decoded parameter types,
/// `Ret` the encoded return type; `Marker` disambiguates the closure
/// shape so one generated parameter accepts every supported form.
///
/// Implemented for closures over [`BamlValue`] parameters returning:
/// - `Ret` (infallible),
/// - `Result<Ret, E>` with `E: BamlValue` — the declared BAML throws
///   class, thrown as that real class,
/// - `Result<Ret, E>` with `E: std::error::Error + Send + Sync` — an
///   arbitrary host error, transported opaquely and rehydratable via
///   [`crate::Error::downcast_ref`] on the same host,
///
/// and the async (`Future`-returning) forms of all three.
pub trait HostCallback<Args, Ret, Marker>: Send + Sync + 'static {
    /// The BAML-level error type this closure surfaces: `Infallible` for an
    /// infallible closure, the declared `E` for a typed-throw closure, or
    /// [`HostCallable`] for an opaque host error. A generated binding whose
    /// BAML `throws` is the callback's inferred error param realizes that
    /// param as this type, so its result is `Error<Cb::Throws>`.
    type Throws: BamlValue;
    #[doc(hidden)]
    fn erase(self, params: &'static [HostParam]) -> Arc<ErasedCallable>;
}

/// Marker types selecting a [`HostCallback`] impl family. Never
/// constructed — they exist so the closure-shape impls do not overlap.
/// A closure resolves its marker by inference; ambiguity can only arise
/// for an error type that is both a BAML value and a `std::error::Error`,
/// which generated classes never are.
pub mod markers {
    pub struct Sync;
    pub struct SyncTyped;
    pub struct SyncOpaque;
    pub struct Async;
    pub struct AsyncTyped;
    pub struct AsyncOpaque;
}

/// Box a per-family dispatch closure into the registry's [`ErasedCallable`].
fn erased(
    f: impl Fn(Vec<wire::BamlToHostArg>) -> Result<DispatchFuture, String> + Send + Sync + 'static,
) -> Arc<ErasedCallable> {
    Arc::new(ErasedCallable(Box::new(f)))
}

/// Convert a fallible closure's `Err` into the wire throw for its family.
fn typed_throw<E: BamlValue>(error: &E) -> HostThrow {
    HostThrow::Typed(error.to_baml())
}

fn opaque_throw<E: std::error::Error + Send + Sync + 'static>(error: E) -> HostThrow {
    HostThrow::Opaque {
        // `class_name` is best-effort debug metadata. Rust has no native
        // leaf type name (unlike python's `type(e).__name__`); the only
        // reflection is `type_name`, whose exact string the compiler does
        // not guarantee — so it is not a reliable `catch` key (use a typed
        // throw for control flow). Sent verbatim: the full path is the
        // most exact form and any leaf-extraction would only lose info.
        class_name: std::any::type_name::<E>().to_string(),
        message: error.to_string(),
        original: Arc::new(error),
    }
}

macro_rules! impl_host_callback {
    ($($A:ident)*) => {
        #[allow(non_snake_case, unused_variables, unused_mut)]
        impl<F, R, $($A,)*> HostCallback<($($A,)*), R, markers::Sync> for F
        where
            F: Fn($($A),*) -> R + Send + Sync + 'static,
            R: BamlValue,
            $($A: BamlValue + Send + 'static,)*
        {
            type Throws = std::convert::Infallible;
            fn erase(self, params: &'static [HostParam]) -> Arc<ErasedCallable> {
                erased(move |args| {
                    let mut slots = slot_args(params, args)?.into_iter();
                    $(let $A = $A::from_baml(slots.next().unwrap_or_default())
                        .map_err(|e| e.to_string())?;)*
                    let value = self($($A),*);
                    Ok(Box::pin(std::future::ready(Ok(value.to_baml()))))
                })
            }
        }

        #[allow(non_snake_case, unused_variables, unused_mut)]
        impl<F, R, E, $($A,)*> HostCallback<($($A,)*), R, markers::SyncTyped> for F
        where
            F: Fn($($A),*) -> Result<R, E> + Send + Sync + 'static,
            R: BamlValue,
            E: BamlValue,
            $($A: BamlValue + Send + 'static,)*
        {
            type Throws = E;
            fn erase(self, params: &'static [HostParam]) -> Arc<ErasedCallable> {
                erased(move |args| {
                    let mut slots = slot_args(params, args)?.into_iter();
                    $(let $A = $A::from_baml(slots.next().unwrap_or_default())
                        .map_err(|e| e.to_string())?;)*
                    let result = self($($A),*)
                        .map(|value| value.to_baml())
                        .map_err(|e| typed_throw(&e));
                    Ok(Box::pin(std::future::ready(result)))
                })
            }
        }

        #[allow(non_snake_case, unused_variables, unused_mut)]
        impl<F, R, E, $($A,)*> HostCallback<($($A,)*), R, markers::SyncOpaque> for F
        where
            F: Fn($($A),*) -> Result<R, E> + Send + Sync + 'static,
            R: BamlValue,
            E: std::error::Error + Send + Sync + 'static,
            $($A: BamlValue + Send + 'static,)*
        {
            type Throws = HostCallable;
            fn erase(self, params: &'static [HostParam]) -> Arc<ErasedCallable> {
                erased(move |args| {
                    let mut slots = slot_args(params, args)?.into_iter();
                    $(let $A = $A::from_baml(slots.next().unwrap_or_default())
                        .map_err(|e| e.to_string())?;)*
                    let result = self($($A),*)
                        .map(|value| value.to_baml())
                        .map_err(opaque_throw);
                    Ok(Box::pin(std::future::ready(result)))
                })
            }
        }

        #[allow(non_snake_case, unused_variables, unused_mut)]
        impl<F, Fut, R, $($A,)*> HostCallback<($($A,)*), R, markers::Async> for F
        where
            F: Fn($($A),*) -> Fut + Send + Sync + 'static,
            Fut: Future<Output = R> + Send + 'static,
            R: BamlValue,
            $($A: BamlValue + Send + 'static,)*
        {
            type Throws = std::convert::Infallible;
            fn erase(self, params: &'static [HostParam]) -> Arc<ErasedCallable> {
                erased(move |args| {
                    let mut slots = slot_args(params, args)?.into_iter();
                    $(let $A = $A::from_baml(slots.next().unwrap_or_default())
                        .map_err(|e| e.to_string())?;)*
                    let future = self($($A),*);
                    Ok(Box::pin(async move { Ok(future.await.to_baml()) }))
                })
            }
        }

        #[allow(non_snake_case, unused_variables, unused_mut)]
        impl<F, Fut, R, E, $($A,)*> HostCallback<($($A,)*), R, markers::AsyncTyped> for F
        where
            F: Fn($($A),*) -> Fut + Send + Sync + 'static,
            Fut: Future<Output = Result<R, E>> + Send + 'static,
            R: BamlValue,
            E: BamlValue,
            $($A: BamlValue + Send + 'static,)*
        {
            type Throws = E;
            fn erase(self, params: &'static [HostParam]) -> Arc<ErasedCallable> {
                erased(move |args| {
                    let mut slots = slot_args(params, args)?.into_iter();
                    $(let $A = $A::from_baml(slots.next().unwrap_or_default())
                        .map_err(|e| e.to_string())?;)*
                    let future = self($($A),*);
                    Ok(Box::pin(async move {
                        future
                            .await
                            .map(|value| value.to_baml())
                            .map_err(|e| typed_throw(&e))
                    }))
                })
            }
        }

        #[allow(non_snake_case, unused_variables, unused_mut)]
        impl<F, Fut, R, E, $($A,)*> HostCallback<($($A,)*), R, markers::AsyncOpaque> for F
        where
            F: Fn($($A),*) -> Fut + Send + Sync + 'static,
            Fut: Future<Output = Result<R, E>> + Send + 'static,
            R: BamlValue,
            E: std::error::Error + Send + Sync + 'static,
            $($A: BamlValue + Send + 'static,)*
        {
            type Throws = HostCallable;
            fn erase(self, params: &'static [HostParam]) -> Arc<ErasedCallable> {
                erased(move |args| {
                    let mut slots = slot_args(params, args)?.into_iter();
                    $(let $A = $A::from_baml(slots.next().unwrap_or_default())
                        .map_err(|e| e.to_string())?;)*
                    let future = self($($A),*);
                    Ok(Box::pin(async move {
                        future
                            .await
                            .map(|value| value.to_baml())
                            .map_err(opaque_throw)
                    }))
                })
            }
        }
    };
}

impl_host_callback!();
impl_host_callback!(A1);
impl_host_callback!(A1 A2);
impl_host_callback!(A1 A2 A3);
impl_host_callback!(A1 A2 A3 A4);
impl_host_callback!(A1 A2 A3 A4 A5);
impl_host_callback!(A1 A2 A3 A4 A5 A6);
impl_host_callback!(A1 A2 A3 A4 A5 A6 A7);
impl_host_callback!(A1 A2 A3 A4 A5 A6 A7 A8);
