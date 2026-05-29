//! End-to-end tests for host-language callables: a host-language callable is
//! bound to an `Object::HostClosure` at the FFI boundary and dispatched
//! through `SysOp::BamlHostCallHostValue` when BAML code invokes it.
//!
//! The tests stand up a fake host dispatcher that:
//! 1. Decodes the engine-side `BamlOutboundValue` args (a list of typed
//!    `BamlOutboundValue`s, per `host_impls::call_host_value`).
//! 2. Looks up the per-key behaviour registered by the test.
//! 3. Calls `sys_native::host_dispatch::complete_with_value` (or
//!    `complete_with_error`) directly with a `BexExternalValue`. This
//!    bypasses the `bridge_cffi` proto-decode path because we already have
//!    the value in-process — the test mirrors what
//!    `bridge_cffi::complete_host_call` does internally, just without the
//!    intermediate `InboundValue` proto round trip.
//!
//! No real bridge is involved — the dispatcher and behaviour table both
//! live in this test binary.

#![allow(unsafe_code)]

mod common;

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

use bex_engine::{
    BexEngine, BexExternalValue, CancellationToken, EngineError, FunctionCallContextBuilder,
};
use bex_resource_types::{HostValueArc, HostValueKind};
use bridge_ctypes::baml_core::cffi::{BamlOutboundValue, baml_outbound_value};
use common::compile_for_engine;
use indexmap::IndexMap;
use prost::Message;
use sys_native::SysOpsExt;
use sys_types::{OpError, OpErrorKind, SysOp};

// ============================================================================
// Fake host dispatch infrastructure
// ============================================================================

/// Result of a fake host invocation.
#[expect(
    clippy::large_enum_variant,
    reason = "test-only enum used as a one-shot return value; allocating a Box per call adds noise without buying anything"
)]
enum FakeReturn {
    /// Complete the call with this successful value.
    Ok(BexExternalValue),
    /// Complete the call with a `HostCallable` error.
    Err { class_name: String, message: String },
    /// Do **not** complete the call — model a hung host. The dispatched
    /// `call_id` is published on [`PENDING_CALL_ID`] so the test can cancel the
    /// BAML call and then assert the in-flight table entry was evicted.
    NeverComplete,
}

// `Arc` (not `Box`) so `global_dispatch` can clone the behaviour out and run
// it without holding the table lock — and so a single host callable can be
// invoked more than once (e.g. as the callback to `xs.map(f)`, which dispatches
// the same key once per element).
type Behaviour = Arc<dyn Fn(Vec<BamlOutboundValue>) -> FakeReturn + Send + Sync>;

static BEHAVIOUR_TABLE: OnceLock<Mutex<HashMap<u64, Behaviour>>> = OnceLock::new();
static NEXT_KEY: AtomicU64 = AtomicU64::new(1);
static DISPATCH_REGISTERED: OnceLock<()> = OnceLock::new();
/// Per-`host_value_key` channel senders used by [`FakeReturn::NeverComplete`]
/// to publish the dispatched `call_id` back to the test that armed the
/// hung-host behaviour. Keyed by `host_value_key` (rather than a single shared
/// slot) because the whole test binary runs in one process under libtest: a
/// single slot would let concurrently-running `NeverComplete` tests clobber
/// each other's sender — dropping it so `recv` sees `Disconnected` — or
/// cross-route a `call_id` into the wrong test's channel.
static PENDING_CALL_ID: OnceLock<Mutex<HashMap<u64, std::sync::mpsc::Sender<u32>>>> =
    OnceLock::new();

fn table() -> &'static Mutex<HashMap<u64, Behaviour>> {
    BEHAVIOUR_TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn pending_call_ids() -> &'static Mutex<HashMap<u64, std::sync::mpsc::Sender<u32>>> {
    PENDING_CALL_ID.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_host_key() -> u64 {
    loop {
        let k = NEXT_KEY.fetch_add(1, Ordering::Relaxed);
        if k != 0 {
            return k;
        }
    }
}

/// One-time install of the C dispatch entry point that delegates to the
/// per-key behaviour table.
fn ensure_dispatch_registered() {
    DISPATCH_REGISTERED.get_or_init(|| {
        sys_native::host_dispatch::set_dispatch_fn(global_dispatch);
    });
}

extern "C" fn global_dispatch(host_value_key: u64, call_id: u32, args: *const u8, length: usize) {
    let bytes: Vec<u8> = if length == 0 || args.is_null() {
        Vec::new()
    } else {
        // SAFETY: the engine passes a slice valid for `length` bytes for the
        // duration of this call.
        unsafe { std::slice::from_raw_parts(args, length) }.to_vec()
    };

    let outbound = match BamlOutboundValue::decode(bytes.as_slice()) {
        Ok(v) => v,
        Err(e) => {
            complete_with_test_error(call_id, "DecodeError", &format!("decode failure: {e}"));
            return;
        }
    };

    let items: Vec<BamlOutboundValue> = match outbound.value {
        Some(baml_outbound_value::Value::ListValue(list)) => list.items,
        other => {
            complete_with_test_error(
                call_id,
                "ProtocolError",
                &format!("expected ListValue in dispatch args, got {other:?}"),
            );
            return;
        }
    };

    // Clone the behaviour out and drop the lock before invoking it; keep the
    // entry so the same key can be dispatched again (reusable callable).
    let behaviour = table().lock().unwrap().get(&host_value_key).cloned();
    let Some(behaviour) = behaviour else {
        complete_with_test_error(
            call_id,
            "KeyError",
            &format!("no registered behaviour for host_value_key {host_value_key}"),
        );
        return;
    };

    match behaviour(items) {
        FakeReturn::Ok(value) => {
            sys_native::host_dispatch::complete_with_value(call_id, value);
        }
        FakeReturn::Err {
            class_name,
            message,
        } => {
            complete_with_test_error(call_id, &class_name, &message);
        }
        FakeReturn::NeverComplete => {
            // Model a hung host: publish the call_id (routed to the test that
            // armed *this* key) and return without completing. The engine's
            // sysop future stays pending until the BAML call is cancelled, at
            // which point the future (and its InflightGuard) is dropped,
            // evicting the table entry.
            if let Some(slot) = PENDING_CALL_ID.get() {
                if let Some(tx) = slot.lock().unwrap().get(&host_value_key) {
                    let _ = tx.send(call_id);
                }
            }
        }
    }
}

fn complete_with_test_error(call_id: u32, class_name: &str, message: &str) {
    sys_native::host_dispatch::complete_with_error(
        call_id,
        OpError::new(
            SysOp::BamlHostCallHostValue,
            OpErrorKind::HostCallable {
                class_name: class_name.to_string(),
                message: message.to_string(),
                traceback: None,
                language: Some("rust".to_string()),
                category: 0,
            },
        ),
    );
}

/// Register a per-key behaviour and return a `HostValueArc` whose key is
/// recognised by `global_dispatch`.
fn register_host_callable<F>(behaviour: F) -> Arc<HostValueArc>
where
    F: Fn(Vec<BamlOutboundValue>) -> FakeReturn + Send + Sync + 'static,
{
    ensure_dispatch_registered();
    let key = next_host_key();
    table().lock().unwrap().insert(key, Arc::new(behaviour));
    HostValueArc::new(key, HostValueKind::Callable)
}

// ============================================================================
// Success path: f(41) returns 42
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_callable_returns_int_result() {
    let source = r#"
        function add_one(f: (int) -> int, x: int) -> int {
            return f(x);
        }
    "#;

    // Behaviour: extract the int arg, return n + 1.
    let arc = register_host_callable(|items| {
        let n = match items.first().and_then(|v| v.value.as_ref()) {
            Some(baml_outbound_value::Value::IntValue(i)) => *i,
            other => {
                return FakeReturn::Err {
                    class_name: "TypeError".to_string(),
                    message: format!("expected int arg, got {other:?}"),
                };
            }
        };
        FakeReturn::Ok(BexExternalValue::Int(n + 1))
    });

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "add_one",
            vec![
                BexExternalValue::HostValue(Arc::clone(&arc)),
                BexExternalValue::Int(41),
            ],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("call_function should succeed");

    match result {
        BexExternalValue::Int(42) => {}
        other => panic!("expected Int(42), got {other:?}"),
    }
    // Hold the Arc until the end of the test so the dispatch table entry
    // is not released early.
    drop(arc);
}

// ============================================================================
// Indirect dispatch: a host callable invoked as a native higher-order-builtin
//         callback. `xs.map(f)` routes each element through the native
//         `Array.map` continuation, which `YieldToCall`s the callback `f`.
//         When `f` is a host callable, that funnels through
//         `execute_call_from_locals_offset` (the indirect path, distinct from a
//         direct `f(x)` `CallIndirect`), which must recognise the `HostClosure`
//         and yield `SysOp::BamlHostCallHostValue`. The host result lands on the
//         stack like any callback's return, resuming the `map` continuation.
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_callable_invoked_from_native_map_continuation() {
    let source = r#"
        function map_double(f: (int) -> int, xs: int[]) -> int[] {
            return xs.map(f);
        }
    "#;

    // Doubles each element; invoked once per array element (so the behaviour
    // must survive repeated dispatch — see `global_dispatch`).
    let arc = register_host_callable(|items| {
        let n = match items.first().and_then(|v| v.value.as_ref()) {
            Some(baml_outbound_value::Value::IntValue(i)) => *i,
            other => {
                return FakeReturn::Err {
                    class_name: "TypeError".to_string(),
                    message: format!("expected int arg, got {other:?}"),
                };
            }
        };
        FakeReturn::Ok(BexExternalValue::Int(n * 2))
    });

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    let xs = BexExternalValue::Array {
        element_type: baml_type::Ty::int(),
        items: vec![
            BexExternalValue::Int(1),
            BexExternalValue::Int(2),
            BexExternalValue::Int(3),
        ],
    };
    let result = engine
        .call_function(
            "map_double",
            vec![BexExternalValue::HostValue(Arc::clone(&arc)), xs],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("call_function should succeed");

    match result {
        BexExternalValue::Array { items, .. } => {
            let got: Vec<i64> = items
                .iter()
                .map(|v| match v {
                    BexExternalValue::Int(i) => *i,
                    other => panic!("expected Int element, got {other:?}"),
                })
                .collect();
            assert_eq!(
                got,
                vec![2, 4, 6],
                "each element must be doubled by the host callable dispatched through map"
            );
        }
        other => panic!("expected Array result, got {other:?}"),
    }
    drop(arc);
}

// ============================================================================
// Operand-layout pin: the VM hand-builds the `SysOp::BamlHostCallHostValue`
//        args as `[handle, args_array, ret_ty]` to match the codegen-generated
//        glue (`sys_ops/.../io_generated.rs` extracts `__arg0` as the handle via
//        `as_owned_but_very_slow`, `__arg1` as the args list, and `__arg2`/
//        `type_arg_0` via `as_baml_type_owned`). Nothing else pins this contract,
//        so a future codegen change to the type-arg operand position (or a swap
//        of any two operands) would silently break host calls. This test makes
//        the contract explicit and fails loudly if the operand order changes:
//
//   * `args[0]` (handle) — proven by the dispatch firing for the *correct*
//     registered key (the behaviour table lookup by `host_value_key` succeeds);
//     if `args[0]` were not the handle, `as_owned_but_very_slow` would not yield
//     a `HostValue` and the sys-op would fail with a `TypeError` before dispatch.
//   * `args[1]` (args array) — asserted to decode to exactly the user args, in
//     order: `[Int(7), Int(8)]`. If `args[1]` carried the ret_ty `Object::Type`
//     instead, the glue's `BexExternalValue::Array` extraction would fail.
//   * `args[2]` (ret_ty) — proven to carry the declared return `Ty` (`int`):
//     the host returns the sum (an `Int`), which only passes return-type
//     validation if `type_arg_0` decoded to `int`. If `args[2]` carried the
//     args array instead, `as_baml_type_owned` would fail and the call would
//     error before reaching the host.
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_callable_sysop_operand_layout_is_pinned() {
    let source = r#"
        function add_two(f: (int, int) -> int, a: int, b: int) -> int {
            return f(a, b);
        }
    "#;

    // Behaviour: assert the decoded args array (`SysOp` operand `args[1]`) is
    // exactly `[Int(7), Int(8)]` in order, then return their sum. Returning an
    // `Int` only survives return-type validation if operand `args[2]` (ret_ty)
    // decoded to the declared `int`.
    let arc = register_host_callable(|items| {
        assert_eq!(
            items.len(),
            2,
            "args[1] (args array) must decode to exactly the two user args; got {} items",
            items.len()
        );
        let decode_int = |v: &BamlOutboundValue| match v.value.as_ref() {
            Some(baml_outbound_value::Value::IntValue(i)) => *i,
            other => panic!("expected IntValue in args array, got {other:?}"),
        };
        let a = decode_int(&items[0]);
        let b = decode_int(&items[1]);
        assert_eq!(a, 7, "args[1][0] must be the first user arg (7), got {a}");
        assert_eq!(b, 8, "args[1][1] must be the second user arg (8), got {b}");
        FakeReturn::Ok(BexExternalValue::Int(a + b))
    });

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "add_two",
            vec![
                BexExternalValue::HostValue(Arc::clone(&arc)),
                BexExternalValue::Int(7),
                BexExternalValue::Int(8),
            ],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("call_function should succeed");

    match result {
        // The result reaching us as Int(15) proves `args[2]` (ret_ty) decoded to
        // `int`: had the type-arg operand been displaced, return-type validation
        // would have rejected the Int or the sys-op would have errored on
        // `as_baml_type_owned` before the host ever ran.
        BexExternalValue::Int(15) => {}
        other => panic!("expected Int(15), got {other:?}"),
    }
    drop(arc);
}

// ============================================================================
// Wrong-return-type path: host returns String where int is declared,
//         surfaces as a catchable `root.errors.HostCallable` instance.
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_callable_wrong_return_type_surfaces_as_host_callable_throw() {
    let source = r#"
        function add_one(f: (int) -> int, x: int) -> int {
            return f(x);
        }
    "#;

    // Behaviour: return a string where an int is expected.
    let arc = register_host_callable(|_items| {
        FakeReturn::Ok(BexExternalValue::String("not-an-int".to_string().into()))
    });

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "add_one",
            vec![
                BexExternalValue::HostValue(Arc::clone(&arc)),
                BexExternalValue::Int(41),
            ],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    match result {
        Err(EngineError::UnhandledThrow { value, .. }) => match value.as_ref() {
            BexExternalValue::Instance { class_name, fields } => {
                assert_eq!(
                    class_name, "baml.errors.HostCallable",
                    "expected baml.errors.HostCallable instance, got {class_name}"
                );
                // The `message` field carries the type-mismatch diagnostic and
                // must round-trip onto the surfaced instance.
                match fields.get("message") {
                    Some(BexExternalValue::String(m)) => {
                        assert!(
                            m.contains("string") && m.contains("int"),
                            "message should describe the int/string mismatch, got {m:?}"
                        );
                    }
                    other => panic!("expected a non-empty message field, got {other:?}"),
                }
            }
            other => panic!("expected Instance, got {other:?}"),
        },
        other => panic!("expected UnhandledThrow(HostCallable), got {other:?}"),
    }
    drop(arc);
}

/// Assert that a host-callable error surfaces as a structured
/// `root.errors.HostCallable` throw.
///
/// Current behaviour: the error rides the sys-op error path in `bex_engine`,
/// which returns `EngineError::UnhandledThrow` directly — exiting the thread
/// event loop WITHOUT re-entering the VM. So although the param's `throws
/// root.errors.HostCallable` *type-checks* and an in-BAML `try/catch`
/// *compiles*, a user `catch` clause does NOT actually run today; the throw
/// surfaces to the host as this `UnhandledThrow`. It becomes catchable from
/// in-BAML `try/catch` once sys-op calls are awaited explicitly. This assertion
/// therefore pins the *current* contract, not the desired end state — see the
/// matching comment at the sys-op error site in `crates/bex_engine/src/lib.rs`.
fn assert_host_callable_throw(result: &Result<BexExternalValue, EngineError>) {
    match result {
        Err(EngineError::UnhandledThrow { value, .. }) => match value.as_ref() {
            BexExternalValue::Instance { class_name, fields } => {
                assert_eq!(
                    class_name, "baml.errors.HostCallable",
                    "expected baml.errors.HostCallable instance, got {class_name}"
                );
                // The diagnostic must reach the surfaced instance's `message`.
                assert!(
                    matches!(
                        fields.get("message"),
                        Some(BexExternalValue::String(m)) if !m.is_empty()
                    ),
                    "expected a non-empty message field, got {:?}",
                    fields.get("message")
                );
            }
            other => panic!("expected Instance, got {other:?}"),
        },
        other => panic!("expected UnhandledThrow(HostCallable), got {other:?}"),
    }
}

// ============================================================================
// Class field-type mismatch: the declared return class has an `int`
//         field, but the host fills it with a string. The shared FFI-boundary
//         guard validates class-*name* identity; the engine-side schema-aware
//         check rejects the wrong field *type*. Surfaces as `HostCallable`.
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_callable_wrong_class_field_type_surfaces_as_host_callable_throw() {
    let source = r#"
        class Point {
            x int
            y int
        }
        function make_point(f: () -> Point) -> Point {
            return f();
        }
    "#;

    // Behaviour: return a Point whose `x` field holds a string (declared int).
    let arc = register_host_callable(|_items| {
        let mut fields = IndexMap::new();
        fields.insert(
            "x".to_string(),
            BexExternalValue::String("oops".to_string().into()),
        );
        fields.insert("y".to_string(), BexExternalValue::Int(2));
        FakeReturn::Ok(BexExternalValue::Instance {
            class_name: "Point".to_string(),
            fields,
        })
    });

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "make_point",
            vec![BexExternalValue::HostValue(Arc::clone(&arc))],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    assert_host_callable_throw(&result);
    drop(arc);
}

// ============================================================================
// Cancel eviction: a hung host call leaves an in-flight `CompletionHandle`
//        in the `host_dispatch` table; cancelling the BAML call drops the
//        sysop future (and its `InflightGuard`), which must evict the entry so
//        it does not leak. A leaked entry + a wrapping u32 call-id is the
//        hazard (a stale late completion resolving a different live call).
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_callable_cancel_evicts_in_flight_entry() {
    let source = r#"
        function add_one(f: (int) -> int, x: int) -> int {
            return f(x);
        }
    "#;

    // Wire a channel so the hung-host behaviour can publish the dispatched
    // call_id back to us.
    let (tx, rx) = std::sync::mpsc::channel::<u32>();

    // Behaviour: never complete — model a hung host.
    let arc = register_host_callable(|_items| FakeReturn::NeverComplete);
    pending_call_ids().lock().unwrap().insert(arc.key, tx);

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    let cancel = CancellationToken::new();
    let ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_cancel_token(cancel.clone())
        .build();

    let engine_for_call = Arc::clone(&engine);
    let arc_for_call = Arc::clone(&arc);
    let call = tokio::spawn(async move {
        engine_for_call
            .call_function(
                "add_one",
                vec![
                    BexExternalValue::HostValue(arc_for_call),
                    BexExternalValue::Int(41),
                ],
                ctx,
                true,
            )
            .await
    });

    // Wait until the host call has been dispatched (entry is now in-flight).
    let call_id = tokio::task::spawn_blocking(move || {
        rx.recv_timeout(std::time::Duration::from_secs(10))
            .expect("host dispatch should fire within 10s")
    })
    .await
    .expect("join spawn_blocking");

    // Cancel the BAML call → engine drops the sysop future → InflightGuard drop
    // → entry evicted.
    cancel.cancel();
    let result = call.await.expect("join call task");
    assert!(
        matches!(result, Err(EngineError::UnhandledThrow { .. })),
        "cancelled call should surface as an UnhandledThrow, got {result:?}"
    );

    // The in-flight entry must be gone. If it leaked, `take` would return the
    // still-present `CompletionHandle`.
    assert!(
        sys_native::host_dispatch::take(call_id).is_none(),
        "cancel must evict the in-flight entry (no leak); call_id {call_id}"
    );

    // Tidy up the global channel slot so it does not dangle into other tests.
    pending_call_ids().lock().unwrap().remove(&arc.key);
    drop(arc);
}

// ============================================================================
// Enum identity mismatch: the declared return is enum `Color`, but the
//         host returns a `Variant` of a different enum `Status`. Rejected by
//         the strict enum-identity check (shared FFI-boundary guard).
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_callable_wrong_enum_identity_surfaces_as_host_callable_throw() {
    let source = r#"
        enum Color {
            Red
            Green
        }
        enum Status {
            Ok
            Bad
        }
        function pick_color(f: () -> Color) -> Color {
            return f();
        }
    "#;

    // Behaviour: return a variant of the WRONG enum (`Status.Ok` for `Color`).
    let arc =
        register_host_callable(|_items| FakeReturn::Ok(BexExternalValue::variant("Status", "Ok")));

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "pick_color",
            vec![BexExternalValue::HostValue(Arc::clone(&arc))],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    assert_host_callable_throw(&result);
    drop(arc);
}

// ============================================================================
// Function-typed return: the callback's declared return type is itself a
//         callable (`() -> int`) and the host returns a `HostValue`. The engine
//         can't materialize a *returned* callable (the result-push has no
//         declared type to bind an `Object::HostClosure`), so the validator
//         rejects it as a structured, catchable `HostCallable` rather than
//         letting it die downstream as a raw `CannotConvert`.
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_callable_returning_a_callable_is_rejected() {
    let source = r#"
        function call_returns_callable(f: (int) -> (() -> int), x: int) -> int {
            f(x);
            return 0;
        }
    "#;

    // The callback returns *another* host callable (a function-typed value).
    let returned = HostValueArc::new(next_host_key(), HostValueKind::Callable);
    let returned_for_cb = Arc::clone(&returned);
    let arc = register_host_callable(move |_items| {
        FakeReturn::Ok(BexExternalValue::HostValue(Arc::clone(&returned_for_cb)))
    });

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "call_returns_callable",
            vec![
                BexExternalValue::HostValue(Arc::clone(&arc)),
                BexExternalValue::Int(1),
            ],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    // Surfaces as a catchable `HostCallable` (not a raw `CannotConvert`), and
    // the message explains that returning a callable is unsupported.
    match result {
        Err(EngineError::UnhandledThrow { value, .. }) => match value.as_ref() {
            BexExternalValue::Instance { class_name, fields } => {
                assert_eq!(class_name, "baml.errors.HostCallable");
                match fields.get("message") {
                    Some(BexExternalValue::String(m)) => assert!(
                        m.contains("callable"),
                        "message should explain the unsupported callable return, got {m:?}"
                    ),
                    other => panic!("expected a message field, got {other:?}"),
                }
            }
            other => panic!("expected Instance, got {other:?}"),
        },
        other => panic!("expected UnhandledThrow(HostCallable), got {other:?}"),
    }
    drop(arc);
    drop(returned);
}

// ============================================================================
// A moving GC during the host-call await must not invalidate return-type
//         validation. The engine captures the declared return type as an owned
//         `Ty` before the await; if it instead re-read the raw `args[2]` heap
//         pointer afterward, a GC during the await could relocate/collect that
//         `Object::Type` (the engine-local `args` Vec is not a GC root and is
//         never forwarded), so schema validation would read a dangling pointer
//         and be silently skipped — wrongly accepting a type-violating return.
//         Here we force a GC while the call is parked on the await, then
//         complete it with a wrong-typed value, and assert it is still rejected.
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_call_ret_ty_survives_gc_during_await() {
    let source = r#"
        function add_one(f: (int) -> int, x: int) -> int {
            return f(x);
        }
    "#;

    // Publish the dispatched call_id so the test controls GC-vs-completion order.
    let (tx, rx) = std::sync::mpsc::channel::<u32>();

    // Park the host call (don't complete) so we can run a GC during the await.
    let arc = register_host_callable(|_items| FakeReturn::NeverComplete);
    pending_call_ids().lock().unwrap().insert(arc.key, tx);

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    let engine_for_call = Arc::clone(&engine);
    let arc_for_call = Arc::clone(&arc);
    let call = tokio::spawn(async move {
        engine_for_call
            .call_function(
                "add_one",
                vec![
                    BexExternalValue::HostValue(arc_for_call),
                    BexExternalValue::Int(41),
                ],
                FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                true,
            )
            .await
    });

    // Wait until the host call is dispatched (engine is now parked on the await).
    let call_id = tokio::task::spawn_blocking(move || {
        rx.recv_timeout(std::time::Duration::from_secs(10))
            .expect("host dispatch should fire within 10s")
    })
    .await
    .expect("join spawn_blocking");

    // Force a moving GC while the engine is parked — relocates the `ret_ty`
    // `Object::Type` that `args[2]` points at.
    engine
        .collect_garbage(bex_heap::CollectionLevel::Major)
        .await;

    // Complete the parked call with a TYPE-VIOLATING value (a string where `int`
    // is declared). With the fix, the pre-captured `ret_ty` drives schema
    // validation → rejected. Without it, the GC-relocated `args[2]` makes
    // validation a silent no-op and the string is wrongly accepted.
    sys_native::host_dispatch::complete_with_value(
        call_id,
        BexExternalValue::String("not-an-int".to_string().into()),
    );

    let result = call.await.expect("join call task");
    assert_host_callable_throw(&result);

    pending_call_ids().lock().unwrap().remove(&arc.key);
    drop(arc);
}

// ============================================================================
// A host callable that throws inside a BAML `spawn`ed body must settle the child
//         future (errored) so the awaiting parent resolves with the throw rather
//         than blocking forever. If the sys-op error path returned an unhandled
//         throw without settling the child future, the child `Future` would stay
//         Pending and `await` would hang the whole call.
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_callable_throw_in_spawn_settles_child_does_not_hang() {
    let source = r#"
        function spawn_and_await(f: (int) -> int, x: int) -> int {
            let fut = spawn { f(x) };
            await fut
        }
    "#;

    // The host callable throws; the child must settle (errored), not hang.
    let arc = register_host_callable(|_items| FakeReturn::Err {
        class_name: "ValueError".to_string(),
        message: "boom from host".to_string(),
    });

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    let call = engine.call_function(
        "spawn_and_await",
        vec![
            BexExternalValue::HostValue(Arc::clone(&arc)),
            BexExternalValue::Int(1),
        ],
        FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        true,
    );

    // Must resolve, not hang. Pre-fix the child future never settles and this
    // times out.
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), call)
        .await
        .expect("spawn + throwing host callable hung — child future never settled");

    assert_host_callable_throw(&result);
    drop(arc);
}

// ============================================================================
// A host callable bound to a generic function-typed parameter is rejected at
//         call setup. A generic parameter's type variables erase to `Ty::Void`
//         at runtime, which the return validator treats as "accept anything" —
//         so the host could return a value of any type into a position BAML
//         treats as the instantiated type. Rather than admit that unvalidatable
//         return, binding the callable fails up front.
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_callable_with_generic_return_is_rejected() {
    let source = r#"
        function apply_generic<T>(f: (T) -> T, x: T) -> T {
            return f(x);
        }
    "#;

    // The callable itself is well-formed; the rejection is about its declared
    // (generic) return type, not its behaviour.
    let arc = register_host_callable(|_items| FakeReturn::Ok(BexExternalValue::Int(1)));

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "apply_generic",
            vec![
                BexExternalValue::HostValue(Arc::clone(&arc)),
                BexExternalValue::Int(1),
            ],
            FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_type_args(vec![baml_type::Ty::int()])
                .build(),
            true,
        )
        .await;

    // Bind-time rejection: the generic/erased return type can't be validated.
    assert!(
        matches!(result, Err(EngineError::TypeMismatch { .. })),
        "a generic-return host callable must be rejected at bind, got {result:?}"
    );
    drop(arc);
}
