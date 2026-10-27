//! Host-value registry and BAML→host callable dispatch.
//!
//! When Rust passes a closure as an argument to a BAML function, the
//! generated binding erases it through [`HostCallback`] and registers it
//! here, sending `InboundValue::Handle{key, HOST_VALUE_CALLABLE}` on the
//! wire. The engine binds the handle to an `Object::HostClosure`; when
//! BAML invokes it, the engine fires the process-global dispatch callback
//! registered via the C ABI, which lands in [`host_dispatch_callback`]:
//! decode the `BamlToHostCall` args, run the user closure on the bridge's
//! own dispatch runtime, and complete the call with a `HostCallCompletion`
//! via `complete_host_call`.
//!
//! A callback completes with exactly one outcome:
//! - `returned`: its result, encoded against the declared return type;
//! - `thrown`: a BAML value — the declared throws class (`E: BamlValue`),
//!   encoded against the declared throws type so a BAML `catch` matches it,
//!   or a BAML error or panic the closure forwards from a nested call
//!   ([`crate::Error`] or [`ThrownValue`]), sent as the engine gave it: a
//!   panic stays a panic;
//! - `raised`: an arbitrary host error (`E: std::error::Error`), registered
//!   here as an opaque entry; the engine builds the
//!   `baml.errors.HostCallable` whose `_handle` references it. If BAML
//!   re-throws it back out, it decodes as the normal throws member
//!   [`HostCallable`], whose `_handle` resolves through the registry back to
//!   the original value ([`HostCallable::downcast_ref`]) — the same-host
//!   round trip;
//! - `bridge_failure`: the bridge's own fault (an unknown key, arguments that
//!   do not decode or do not match the declared parameters, a value too deep
//!   for the wire, a handle that cannot be cloned, a panicking closure, a
//!   callback cancelled before it ran), which the engine surfaces as an SDK
//!   panic rather than a catchable throw.
//!
//! The engine releases registry entries through the release callback at
//! its safepoints.

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
    BamlValue, SdkError,
    baml_value::internal::__BamlValuePrivate,
    capi,
    encode::{EncodeError, Encoder},
    error::ThrownValue,
    loader::log,
    wire::{self, host_call_completion::Outcome, inbound_value::Value as In},
};

/// One declared parameter of a BAML callable type, as the generated
/// binding describes it to the dispatcher: required params arrive
/// positionally, optional params by name (absent when BAML omits them).
#[derive(Debug, Clone, Copy)]
pub struct HostParam {
    pub name: &'static str,
    pub optional: bool,
}

/// The function type a host callable is sent under: its parameters, the
/// return type its results fill, and the throws type its thrown values fill
/// (`unknown` for a callback whose effect the caller's closure decides).
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct CallableTy {
    params: Vec<wire::BamlTyFunctionParam>,
    ret: wire::BamlTy,
    throws: wire::BamlTy,
}

impl CallableTy {
    pub fn new(
        params: Vec<wire::BamlTyFunctionParam>,
        ret: wire::BamlTy,
        throws: wire::BamlTy,
    ) -> Self {
        Self {
            params,
            ret,
            throws,
        }
    }

    /// The type as the wire states it.
    pub(crate) fn to_wire(&self) -> wire::BamlTy {
        crate::baml_value::internal::function_ty(
            self.params.clone(),
            self.ret.clone(),
            self.throws.clone(),
        )
    }
}

/// How a value a callback fails with crosses back into the engine: see
/// [`__BamlValuePrivate::throw_in`](crate::baml_value::internal::__BamlValuePrivate::throw_in).
#[doc(hidden)]
pub enum HostThrow {
    /// A BAML value, encoded against the callable's declared throws type.
    Thrown(wire::InboundValue),
    /// An exception of the host's own: the engine builds the
    /// `baml.errors.HostCallable` around its registration.
    Raised(Raise),
}

/// An exception of the host's own, raised by a callback.
#[doc(hidden)]
pub struct Raise {
    class_name: String,
    message: String,
    original: Arc<dyn Any + Send + Sync>,
}

/// One callback's completion: its outcome, and the encoding whose creations
/// the engine takes over once the outcome is delivered.
#[doc(hidden)]
pub struct Completion {
    outcome: Outcome,
    encoder: Encoder,
}

impl Completion {
    /// The callback's result, encoded against the declared return type.
    fn returned<R: BamlValue>(value: &R, ret: &wire::BamlTy) -> Self {
        match crate::encode::callback_value(|encoder| value.to_baml_in(ret, encoder)) {
            Ok((value, encoder)) => Self {
                outcome: Outcome::Returned(value),
                encoder,
            },
            Err(error) => {
                Self::bridge_failure(format!("the callback's result cannot be sent: {error}"))
            }
        }
    }

    /// The callback's declared BAML error, thrown against the declared
    /// throws type (or raised, for a host error the runtime transports).
    fn thrown<E: BamlValue>(error: &E, throws: &wire::BamlTy) -> Self {
        let mut encoder = Encoder::new();
        match error.throw_in(throws, &mut encoder) {
            Ok(HostThrow::Thrown(value)) => {
                match crate::encode::fits_the_wire(value, crate::encode::RESULT_DEPTH) {
                    Ok(value) => Self {
                        outcome: Outcome::Thrown(value),
                        encoder,
                    },
                    Err(error) => Self::unsendable_throw(&error),
                }
            }
            Ok(HostThrow::Raised(raise)) => Self::raised(raise, encoder),
            Err(error) => Self::unsendable_throw(&error),
        }
    }

    /// An error of the host's own type. A BAML error or panic the callback
    /// forwards — a [`crate::Error`] from a nested call, whose source is the
    /// value BAML threw, or that [`ThrownValue`] itself — goes back as
    /// thrown, exactly as the engine sent it: a panic stays a panic and an
    /// error keeps its class. Anything else is raised.
    fn host_error<E: std::error::Error + Send + Sync + 'static>(error: E) -> Self {
        let forwarded = {
            let error: &(dyn std::error::Error + 'static) = &error;
            error
                .downcast_ref::<ThrownValue>()
                .or_else(|| error.source()?.downcast_ref::<ThrownValue>())
                .map(|thrown| {
                    crate::encode::callback_value(|encoder| thrown.value().to_baml(encoder))
                })
        };
        match forwarded {
            Some(Ok((value, encoder))) => Self {
                outcome: Outcome::Thrown(value),
                encoder,
            },
            Some(Err(error)) => Self::unsendable_throw(&error),
            None => Self::raised(
                Raise {
                    // `class_name` is best-effort debug metadata. Rust has no
                    // native leaf type name (unlike python's
                    // `type(e).__name__`); the only reflection is
                    // `type_name`, whose exact string the compiler does not
                    // guarantee — so it is not a reliable `catch` key (use a
                    // typed throw for control flow). Sent verbatim: the full
                    // path is the most exact form and any leaf-extraction
                    // would only lose info.
                    class_name: std::any::type_name::<E>().to_string(),
                    message: error.to_string(),
                    original: Arc::new(error),
                },
                Encoder::new(),
            ),
        }
    }

    fn raised(raise: Raise, mut encoder: Encoder) -> Self {
        let handle = insert(raise.original);
        encoder.registered(handle);
        Self {
            outcome: Outcome::Raised(wire::HostException {
                message: raise.message,
                class_name: raise.class_name,
                language: "rust".to_string(),
                traceback: None,
                handle,
            }),
            encoder,
        }
    }

    fn unsendable_throw(error: &EncodeError) -> Self {
        Self::bridge_failure(format!(
            "the callback's thrown value cannot be sent: {error}"
        ))
    }

    /// A failure of the bridge's own: the engine surfaces it as an SDK
    /// panic, not a catchable throw.
    fn bridge_failure(message: String) -> Self {
        Self {
            outcome: Outcome::BridgeFailure(message),
            encoder: Encoder::new(),
        }
    }
}

#[doc(hidden)]
pub type DispatchFuture = Pin<Box<dyn Future<Output = Completion> + Send>>;

/// An erased host closure: decode the incoming args, run the user closure,
/// produce its completion. The `Err` arm is an argument fault (arguments
/// that do not match the declared parameters or do not decode) — a bridge
/// fault, not a user throw. Public only through [`HostCallback::erase`]'s
/// signature.
#[doc(hidden)]
pub type ErasedCall =
    Box<dyn Fn(Vec<wire::BamlToHostArg>) -> Result<DispatchFuture, String> + Send + Sync>;

/// A registered host closure. A concrete type (not a `dyn Fn` alias) so the
/// registry can hold it as `dyn Any` and downcast it back.
struct ErasedCallable {
    call: ErasedCall,
    /// The function type the callable was sent under. A callable has no
    /// type of its own on the wire, so when the engine hands its handle
    /// back, sending it again states this type.
    function_type: wire::BamlTy,
}

struct Registry {
    next_key: AtomicU64,
    /// Every host value — a registered [`ErasedCallable`] or an opaque
    /// original error — is stored type-erased. Each key has exactly one
    /// use site (fixed at registration: a dispatch key is always a
    /// callable, a raise's handle always an error), which downcasts to
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
// BUG: a callback's result is dropped on these threads' default 2 MiB
// stacks, and `serde_json::Value` drops its children recursively: a `Json`
// result nested deeper than roughly 10,000 levels (far past anything the wire
// carries, so already refused) overflows the stack while it is dropped.
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

/// Drop a registration the engine never received: the value it was
/// registered for was never sent.
pub(crate) fn unregister(key: u64) {
    host_release_callback(key);
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

/// Register `cb` (via its [`HostCallback`] erasure) with `encoder`, which
/// unregisters it if the value is never sent, and return the wire value the
/// generated binding sends for the callable argument.
///
/// A host callable has no type of its own: `function_type`, the type its
/// parameter declares (each of the binding's type variables spelled as the
/// Rust type the call binds it to, an open effect as `unknown`), is stated
/// beside its handle, and the engine binds the callable to it. The
/// closure's results fill that type's return slot and its BAML throws fill
/// the throws slot.
pub(crate) fn callable_handle<Args, Ret: BamlValue, Marker, Cb: HostCallback<Args, Ret, Marker>>(
    cb: Cb,
    params: &'static [HostParam],
    function_type: CallableTy,
    encoder: &mut Encoder,
) -> wire::InboundValue {
    let stated = function_type.to_wire();
    let CallableTy { ret, throws, .. } = function_type;
    let key = insert(Arc::new(ErasedCallable {
        call: cb.erase(params, ret, throws),
        function_type: stated.clone(),
    }));
    encoder.registered(key);
    wire::InboundValue {
        value_type: Some(stated),
        value: Some(In::Handle(wire::BamlHandle {
            key,
            handle_type: wire::BamlHandleType::HostValueCallable as i32,
        })),
    }
}

/// The function type a host callable this process registered was sent
/// under, while the engine holds it.
pub(crate) fn registered_function_type(key: u64) -> Option<wire::BamlTy> {
    lookup(key)?
        .downcast::<ErasedCallable>()
        .ok()
        .map(|callable| callable.function_type.clone())
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
            (api.register_host_dispatch_v2)(host_dispatch_callback);
            (api.register_host_cancel_callback)(host_cancel_callback);
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

/// What preparing a callback for its closure came to.
enum Prepared {
    /// The closure ran (a sync family) or built its future (an async one).
    Ready(DispatchFuture),
    /// The engine cancelled the call before the closure ran; the engine
    /// drops whatever completes it.
    Cancelled,
    /// The arguments do not match the declared parameters or do not decode.
    ArgumentFault(String),
}

/// Engine → bridge: BAML invoked a host callable. Copy the borrowed args,
/// resolve the closure, and hand execution to the dispatch runtime; the
/// engine's worker returns immediately and awaits the completion.
extern "C" fn host_dispatch_callback(args: *const u8, length: usize) {
    if args.is_null() || length > isize::MAX as usize {
        return;
    }
    // SAFETY: the runtime lends V2 envelope bytes for this callback.
    #[expect(unsafe_code)]
    let bytes = unsafe { std::slice::from_raw_parts(args, length) };
    // An envelope that does not decode names no call to complete.
    let Ok(mut envelope) = wire::HostInvocation::decode(bytes) else {
        return;
    };
    let host_value_key = envelope.host_value_key;
    let call_id = envelope.callback_id;
    let bytes = std::mem::take(&mut envelope.application_args);
    let frame = Arc::new(CallbackFrame {
        envelope,
        cancelled: std::sync::atomic::AtomicBool::new(false),
    });
    CALLBACK_FRAMES
        .lock()
        .unwrap()
        .insert(call_id, Arc::downgrade(&frame));

    let call = match wire::BamlToHostCall::decode(bytes.as_slice()) {
        Ok(call) => call,
        Err(e) => {
            log::warn(&format!("malformed BamlToHostCall payload: {e}"));
            complete_bridge_failure(call_id, format!("malformed BamlToHostCall payload: {e}"));
            return;
        }
    };

    // Resolve before spawning so a missing entry surfaces as a bridge
    // failure now — the engine holding a handle the bridge cannot find is
    // a bridge bug, not a user error.
    let Some(callable) = lookup(host_value_key).and_then(|v| v.downcast::<ErasedCallable>().ok())
    else {
        log::warn(&format!(
            "no host callable registered for key {host_value_key}"
        ));
        release_args(call.args);
        complete_bridge_failure(
            call_id,
            "no host callable is registered for the dispatched key".to_string(),
        );
        return;
    };

    let handle = DISPATCH_RT.handle().clone();
    DISPATCH_RT.spawn(async move {
        // Run the erased closure on the blocking pool: for the sync families it
        // runs the user closure body inline, so this both isolates a panic (as
        // a `JoinError` that still completes the call) and keeps a slow/blocking
        // sync closure off the 2-worker async pool. For the async families it
        // only builds the future, which is driven on the async pool below.
        let sync_frame = frame.clone();
        let prepared = handle
            .spawn_blocking(move || {
                if sync_frame.cancelled.load(Ordering::Acquire) {
                    release_args(call.args);
                    return Prepared::Cancelled;
                }
                let _scope = SyncFrameScope::enter(sync_frame);
                let _dispatch = BlockingDispatchScope::enter();
                match (callable.call)(call.args) {
                    Ok(future) => Prepared::Ready(future),
                    Err(message) => Prepared::ArgumentFault(message),
                }
            })
            .await;
        let future = match prepared {
            Ok(Prepared::Ready(future)) => future,
            Ok(Prepared::Cancelled) => {
                complete_bridge_failure(
                    call_id,
                    "the host callable was cancelled before it ran".to_string(),
                );
                return;
            }
            Ok(Prepared::ArgumentFault(message)) => {
                log::warn(&format!("host callable arguments refused: {message}"));
                complete_bridge_failure(
                    call_id,
                    format!("host callable arguments refused: {message}"),
                );
                return;
            }
            Err(join_error) => {
                log::warn(&format!(
                    "host callable panicked during invocation: {join_error}"
                ));
                complete_bridge_failure(
                    call_id,
                    format!("host callable panicked during invocation: {join_error}"),
                );
                return;
            }
        };
        match handle.spawn(ASYNC_FRAME.scope(frame, future)).await {
            Ok(completion) => complete(call_id, completion),
            Err(join_error) => {
                log::warn(&format!("host callable panicked: {join_error}"));
                complete_bridge_failure(call_id, format!("host callable panicked: {join_error}"));
            }
        }
    });
}

/// Release the engine references in arguments no closure will take.
fn release_args(args: Vec<wire::BamlToHostArg>) {
    args.into_iter()
        .filter_map(|arg| arg.value)
        .for_each(crate::outbound::release);
}

/// Deliver `completion` to the engine, which then owns what its encoding
/// created.
fn complete(call_id: u32, completion: Completion) {
    let Completion { outcome, encoder } = completion;
    let Ok(api) = capi::api() else {
        // Unreachable in practice: the engine just called us, so it is
        // loaded. Nothing to complete against if not; the encoding's
        // creations are released with it.
        return;
    };
    let bytes = wire::HostCallCompletion {
        outcome: Some(outcome),
    }
    .encode_to_vec();
    // SAFETY: `bytes` is valid for its length for the duration of the
    // call; the engine copies before returning.
    #[expect(unsafe_code)]
    unsafe {
        (api.complete_host_call)(call_id, bytes.as_ptr().cast(), bytes.len());
    }
    encoder.commit();
}

/// A failure of the bridge's own: the engine surfaces it as an SDK panic,
/// not a catchable throw.
fn complete_bridge_failure(call_id: u32, message: String) {
    complete(call_id, Completion::bridge_failure(message));
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
/// erased default it is the honest `dyn Any + …`. The incoming wire
/// `class_name` is ignored on decode — it is unstable (`type_name` has no
/// format guarantee) and redundant with the TypeId-checked `original`.
///
/// Only the runtime builds a `baml.errors.HostCallable`, around a
/// callback's raised exception: a `HostCallable` cannot be sent as an
/// argument or returned as a value. A callback that fails with one raises
/// its original again, and the runtime builds a fresh instance around it.
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
// [`HostCallback`] families. As a `BamlValue` it takes the typed path,
// whose throw raises the original again (see `throw_in`).
//
// The erased default and the concrete `HostCallable<T>` (below) are
// disjoint `BamlValue` impls: the concrete impl's `T` is implicitly
// `Sized`, which the unsized `dyn Any + Send + Sync` can never satisfy, so
// they never overlap.
impl crate::baml_value::internal::__BamlValuePrivate for HostCallable<dyn Any + Send + Sync> {
    /// The runtime alone builds a `baml.errors.HostCallable` (around a
    /// callback's raised exception): a host never sends one as a value.
    fn to_baml(&self, _encoder: &mut Encoder) -> Result<wire::InboundValue, EncodeError> {
        Err(EncodeError::RuntimeOnly {
            what: "a `baml.errors.HostCallable`",
        })
    }

    /// Thrown by a callback, the host error is raised again: the engine
    /// builds a fresh `baml.errors.HostCallable` around the same original.
    fn throw_in(
        &self,
        _slot: &wire::BamlTy,
        _encoder: &mut Encoder,
    ) -> Result<HostThrow, EncodeError> {
        Ok(HostThrow::Raised(Raise {
            class_name: std::any::type_name::<dyn Any + Send + Sync>().to_string(),
            message: self.message.clone(),
            original: Arc::clone(&self.original),
        }))
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
    /// See the erased impl: a host never sends one as a value.
    fn to_baml(&self, _encoder: &mut Encoder) -> Result<wire::InboundValue, EncodeError> {
        Err(EncodeError::RuntimeOnly {
            what: "a `baml.errors.HostCallable`",
        })
    }

    /// See the erased impl: the original is raised again.
    fn throw_in(
        &self,
        _slot: &wire::BamlTy,
        _encoder: &mut Encoder,
    ) -> Result<HostThrow, EncodeError> {
        Ok(HostThrow::Raised(Raise {
            class_name: std::any::type_name::<T>().to_string(),
            message: self.message.clone(),
            original: Arc::clone(&self.original) as Arc<dyn Any + Send + Sync>,
        }))
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
/// `Option` parameter decodes as `None`. Arguments that do not match the
/// declared parameters — a missing or extra positional, a name no optional
/// parameter has, a name given twice — are refused, and their references
/// released.
fn slot_args(
    params: &[HostParam],
    args: Vec<wire::BamlToHostArg>,
) -> Result<Vec<wire::BamlOutboundValue>, String> {
    let required = params.iter().filter(|param| !param.optional).count();
    let positional = args.iter().filter(|arg| !arg.is_optional_arg).count();
    let fault = if positional == required {
        args.iter()
            .filter(|arg| arg.is_optional_arg)
            .enumerate()
            .find_map(|(index, arg)| {
                if !params
                    .iter()
                    .any(|param| param.optional && param.name == arg.arg_name)
                {
                    Some(format!("no optional parameter is named `{}`", arg.arg_name))
                } else if args
                    .iter()
                    .filter(|other| other.is_optional_arg)
                    .take(index)
                    .any(|other| other.arg_name == arg.arg_name)
                {
                    Some(format!("argument `{}` is given twice", arg.arg_name))
                } else {
                    None
                }
            })
    } else {
        Some(format!(
            "{positional} positional arguments for {required} required parameters"
        ))
    };
    if let Some(fault) = fault {
        release_args(args);
        return Err(fault);
    }
    let mut positional = Vec::with_capacity(required);
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
    Ok(params
        .iter()
        .map(|param| {
            if param.optional {
                named.remove(param.name).unwrap_or_default()
            } else {
                positional.next().unwrap_or_else(|| {
                    unreachable!("one positional argument per required parameter, counted above")
                })
            }
        })
        .collect())
}

/// Decode the next slotted argument as a `T`. A refusal releases the
/// references the arguments after it hold; the arguments before it are
/// owned by their decoded values.
fn next_argument<T: BamlValue>(
    slots: &mut std::vec::IntoIter<wire::BamlOutboundValue>,
) -> Result<T, String> {
    let slot = slots.next().unwrap_or_default();
    T::from_baml(slot).map_err(|error| {
        slots.by_ref().for_each(crate::outbound::release);
        error.to_string()
    })
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
///   [`HostCallable::downcast_ref`] on the same host; a [`crate::Error`]
///   from a nested BAML call, or its [`ThrownValue`], is forwarded to BAML
///   as the value BAML threw,
///
/// and the async (`Future`-returning) forms of all three.
pub trait HostCallback<Args, Ret, Marker>: Send + Sync + 'static {
    /// The BAML-level error type this closure surfaces: `Infallible` for an
    /// infallible closure, the declared `E` for a typed-throw closure, or
    /// [`HostCallable`] for an opaque host error. A generated binding whose
    /// BAML `throws` is the callback's inferred error param realizes that
    /// param as this type, so its result is `Error<Cb::Throws>`.
    type Throws: BamlValue;
    /// Erase the closure behind the dispatcher's uniform signature. `ret`
    /// is the declared return type its results fill, `throws` the declared
    /// throws type its BAML errors fill.
    #[doc(hidden)]
    fn erase(
        self,
        params: &'static [HostParam],
        ret: wire::BamlTy,
        throws: wire::BamlTy,
    ) -> ErasedCall;
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

/// Box a per-family dispatch closure into an [`ErasedCall`].
fn erased(
    f: impl Fn(Vec<wire::BamlToHostArg>) -> Result<DispatchFuture, String> + Send + Sync + 'static,
) -> ErasedCall {
    Box::new(f)
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
            fn erase(
                self,
                params: &'static [HostParam],
                ret: wire::BamlTy,
                _throws: wire::BamlTy,
            ) -> ErasedCall {
                erased(move |args| {
                    let mut slots = slot_args(params, args)?.into_iter();
                    $(let $A = next_argument::<$A>(&mut slots)?;)*
                    let value = self($($A),*);
                    Ok(Box::pin(std::future::ready(Completion::returned(&value, &ret))))
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
            fn erase(
                self,
                params: &'static [HostParam],
                ret: wire::BamlTy,
                throws: wire::BamlTy,
            ) -> ErasedCall {
                erased(move |args| {
                    let mut slots = slot_args(params, args)?.into_iter();
                    $(let $A = next_argument::<$A>(&mut slots)?;)*
                    let completion = match self($($A),*) {
                        Ok(value) => Completion::returned(&value, &ret),
                        Err(error) => Completion::thrown(&error, &throws),
                    };
                    Ok(Box::pin(std::future::ready(completion)))
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
            fn erase(
                self,
                params: &'static [HostParam],
                ret: wire::BamlTy,
                _throws: wire::BamlTy,
            ) -> ErasedCall {
                erased(move |args| {
                    let mut slots = slot_args(params, args)?.into_iter();
                    $(let $A = next_argument::<$A>(&mut slots)?;)*
                    let completion = match self($($A),*) {
                        Ok(value) => Completion::returned(&value, &ret),
                        Err(error) => Completion::host_error(error),
                    };
                    Ok(Box::pin(std::future::ready(completion)))
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
            fn erase(
                self,
                params: &'static [HostParam],
                ret: wire::BamlTy,
                _throws: wire::BamlTy,
            ) -> ErasedCall {
                erased(move |args| {
                    let mut slots = slot_args(params, args)?.into_iter();
                    $(let $A = next_argument::<$A>(&mut slots)?;)*
                    let future = self($($A),*);
                    let ret = ret.clone();
                    Ok(Box::pin(async move { Completion::returned(&future.await, &ret) }))
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
            fn erase(
                self,
                params: &'static [HostParam],
                ret: wire::BamlTy,
                throws: wire::BamlTy,
            ) -> ErasedCall {
                erased(move |args| {
                    let mut slots = slot_args(params, args)?.into_iter();
                    $(let $A = next_argument::<$A>(&mut slots)?;)*
                    let future = self($($A),*);
                    let ret = ret.clone();
                    let throws = throws.clone();
                    Ok(Box::pin(async move {
                        match future.await {
                            Ok(value) => Completion::returned(&value, &ret),
                            Err(error) => Completion::thrown(&error, &throws),
                        }
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
            fn erase(
                self,
                params: &'static [HostParam],
                ret: wire::BamlTy,
                _throws: wire::BamlTy,
            ) -> ErasedCall {
                erased(move |args| {
                    let mut slots = slot_args(params, args)?.into_iter();
                    $(let $A = next_argument::<$A>(&mut slots)?;)*
                    let future = self($($A),*);
                    let ret = ret.clone();
                    Ok(Box::pin(async move {
                        match future.await {
                            Ok(value) => Completion::returned(&value, &ret),
                            Err(error) => Completion::host_error(error),
                        }
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

struct CallbackFrame {
    envelope: wire::HostInvocation,
    cancelled: std::sync::atomic::AtomicBool,
}

static CALLBACK_FRAMES: LazyLock<Mutex<HashMap<u32, std::sync::Weak<CallbackFrame>>>> =
    LazyLock::new(Mutex::default);
thread_local! { static SYNC_FRAME: std::cell::RefCell<Option<Arc<CallbackFrame>>> = const { std::cell::RefCell::new(None) }; }
tokio::task_local! { static ASYNC_FRAME: Arc<CallbackFrame>; }

pub(crate) fn current_invocation_state() -> u64 {
    SYNC_FRAME
        .with(|slot| {
            slot.borrow()
                .as_ref()
                .map(|frame| frame.envelope.effective_state)
        })
        .or_else(|| {
            ASYNC_FRAME
                .try_with(|frame| frame.envelope.effective_state)
                .ok()
        })
        .unwrap_or(0)
}

struct SyncFrameScope(Option<Arc<CallbackFrame>>);
impl SyncFrameScope {
    fn enter(frame: Arc<CallbackFrame>) -> Self {
        Self(SYNC_FRAME.with(|slot| slot.replace(Some(frame))))
    }
}
impl Drop for SyncFrameScope {
    fn drop(&mut self) {
        SYNC_FRAME.with(|slot| slot.replace(self.0.take()));
    }
}

extern "C" fn host_cancel_callback(id: u32) {
    if let Some(frame) = CALLBACK_FRAMES
        .lock()
        .unwrap()
        .get(&id)
        .and_then(std::sync::Weak::upgrade)
    {
        frame.cancelled.store(true, Ordering::Release);
    }
}

impl Drop for CallbackFrame {
    fn drop(&mut self) {
        CALLBACK_FRAMES
            .lock()
            .unwrap()
            .remove(&self.envelope.callback_id);
        // The envelope transferred one owned state reference, and the
        // references its cancel token holds.
        crate::handle::release_engine_key(self.envelope.effective_state);
        if let Some(cancel) = self.envelope.cancel.take() {
            crate::outbound::release(cancel);
        }
    }
}

/// A captured inheritance scope owns state independently of the callback ID.
#[derive(Clone)]
pub struct Invocation(Arc<CallbackFrame>);
impl Invocation {
    /// The invocation the running host callback was called in, captured so
    /// work it spawns can inherit it; `None` outside a callback.
    pub fn current() -> Result<Option<Self>, SdkError> {
        let Some(frame) = SYNC_FRAME
            .with(|slot| slot.borrow().clone())
            .or_else(|| ASYNC_FRAME.try_with(Arc::clone).ok())
        else {
            return Ok(None);
        };
        let refused = || SdkError::new("the running callback's invocation could not be captured");
        let effective_state =
            crate::handle::clone_engine_key(frame.envelope.effective_state).ok_or_else(refused)?;
        let mut encoder = Encoder::new();
        let cancel = match &frame.envelope.cancel {
            Some(cancel) => match crate::outbound::clone(cancel, &mut encoder) {
                Ok(cancel) => Some(cancel),
                Err(crate::encode::CloneRefused) => {
                    crate::handle::release_engine_key(effective_state);
                    return Err(refused());
                }
            },
            None => None,
        };
        // The captured frame owns the clones now.
        encoder.commit();
        Ok(Some(Self(Arc::new(CallbackFrame {
            envelope: wire::HostInvocation {
                effective_state,
                cancel,
                ..Default::default()
            },
            cancelled: std::sync::atomic::AtomicBool::new(false),
        }))))
    }
    /// The invocation's effective cancel token, which owns references of
    /// its own.
    #[doc(hidden)]
    pub fn cancel<T: crate::BamlValue>(&self) -> Result<T, crate::DecodeError> {
        // A frame without a token decodes the null it holds, which no cancel
        // token type accepts.
        let mut encoder = Encoder::new();
        let cancel = match &self.0.envelope.cancel {
            Some(cancel) => crate::outbound::clone(cancel, &mut encoder)?,
            None => wire::BamlOutboundValue::default(),
        };
        encoder.commit();
        T::from_baml(cancel)
    }
    pub fn run<T>(&self, body: impl FnOnce() -> T) -> T {
        let _scope = SyncFrameScope::enter(Arc::clone(&self.0));
        body()
    }
    pub fn scope<F: Future>(&self, future: F) -> impl Future<Output = F::Output> + use<F> {
        ASYNC_FRAME.scope(Arc::clone(&self.0), future)
    }
}

thread_local! { static BLOCKING_DISPATCH: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
pub(crate) fn in_blocking_dispatch() -> bool {
    BLOCKING_DISPATCH.with(std::cell::Cell::get)
}
struct BlockingDispatchScope(bool);
impl BlockingDispatchScope {
    fn enter() -> Self {
        Self(BLOCKING_DISPATCH.with(|slot| slot.replace(true)))
    }
}
impl Drop for BlockingDispatchScope {
    fn drop(&mut self) {
        BLOCKING_DISPATCH.with(|slot| slot.set(self.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARAMS: &[HostParam] = &[
        HostParam {
            name: "x",
            optional: false,
        },
        HostParam {
            name: "y",
            optional: true,
        },
    ];

    fn positional(value: i64) -> wire::BamlToHostArg {
        wire::BamlToHostArg {
            value: Some(int(value)),
            arg_name: String::new(),
            is_optional_arg: false,
        }
    }

    fn named(name: &str, value: i64) -> wire::BamlToHostArg {
        wire::BamlToHostArg {
            value: Some(int(value)),
            arg_name: name.to_string(),
            is_optional_arg: true,
        }
    }

    fn int(value: i64) -> wire::BamlOutboundValue {
        wire::BamlOutboundValue {
            value: Some(wire::baml_outbound_value::Value::IntValue(value)),
        }
    }

    #[test]
    fn arguments_slot_by_position_and_name() {
        assert_eq!(
            slot_args(PARAMS, vec![positional(1), named("y", 2)]),
            Ok(vec![int(1), int(2)])
        );
        // An omitted optional is null.
        assert_eq!(
            slot_args(PARAMS, vec![positional(1)]),
            Ok(vec![int(1), wire::BamlOutboundValue::default()])
        );
    }

    /// Arguments that do not match the declared parameters are the bridge's
    /// fault, never silently dropped.
    #[test]
    fn arguments_that_do_not_match_the_parameters_are_refused() {
        assert_eq!(
            slot_args(PARAMS, vec![]),
            Err("0 positional arguments for 1 required parameters".to_string())
        );
        assert_eq!(
            slot_args(PARAMS, vec![positional(1), positional(2)]),
            Err("2 positional arguments for 1 required parameters".to_string())
        );
        assert_eq!(
            slot_args(PARAMS, vec![positional(1), named("z", 2)]),
            Err("no optional parameter is named `z`".to_string())
        );
        assert_eq!(
            slot_args(PARAMS, vec![positional(1), named("x", 2)]),
            Err("no optional parameter is named `x`".to_string())
        );
        assert_eq!(
            slot_args(PARAMS, vec![positional(1), named("y", 2), named("y", 3)]),
            Err("argument `y` is given twice".to_string())
        );
    }

    /// A `HostCallable` is the runtime's to build: a host never sends one as
    /// a value, and a callback that throws one raises its original again.
    #[test]
    fn host_callables_are_raised_never_encoded() {
        let error = HostCallable {
            message: "boom".to_string(),
            original: Arc::new(7_u8) as Arc<dyn Any + Send + Sync>,
        };
        assert!(matches!(
            error.to_baml(&mut Encoder::new()),
            Err(EncodeError::RuntimeOnly { .. })
        ));
        let slot = <HostCallable as __BamlValuePrivate>::baml_ty();
        let Ok(HostThrow::Raised(raise)) = error.throw_in(&slot, &mut Encoder::new()) else {
            panic!("a HostCallable is raised");
        };
        assert_eq!(raise.message, "boom");
        assert!(Arc::ptr_eq(&raise.original, &error.original));
    }
}
