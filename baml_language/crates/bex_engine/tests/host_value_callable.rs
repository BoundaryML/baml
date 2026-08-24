//! End-to-end tests for host-language callables: a host-language callable is
//! bound to an `Object::HostClosure` at the FFI boundary and dispatched
//! through `SysOp::BamlHostCallHostValue` when BAML code invokes it.
//!
//! The tests stand up a fake host dispatcher that:
//! 1. Decodes the engine-side `BamlToHostCall` (the resolved, declared-order
//!    supplied args, per `host_impls::call_host_value`) and flattens it back to
//!    a positional list.
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
    RuntimeTy,
};
use bex_resource_types::{HostValueArc, HostValueKind};
use bridge_ctypes::baml_bridge::cffi::{BamlOutboundValue, BamlToHostCall, baml_outbound_value};
use common::compile_for_engine;
use indexmap::IndexMap;
use prost::Message;
use sys_native::SysOpsExt;

// ============================================================================
// Feasibility probe: `let x: T = v` / `match (v) { let x: T => x, _ => ... }`
//         in a generic context must (a) compile (exhaustiveness analyzer
//         must not treat generic-typed patterns as exhaustive) and (b) at
//         runtime, test the value's type against the substituted T.
//
// Probe result: **(a) fails for `match`** (E0063 unreachable arm — the
// analyzer treats `let x: T => …` as covering everything). With `if let`
// the compile error goes away (single-arm condition; the else is implicit),
// but **(b) also fails**: the runtime pattern is essentially always-false
// — `if let x: T = v { x } else { fallback }` with T=int and v=Int(42)
// returns `fallback`. So generic-typed patterns don't perform the runtime
// type test against the substituted T; monomorphization isn't reaching the
// pattern-compilation path.
//
// Implication: a BAML-side wrapper for `call_host_value` that uses
// pattern matching to do its return-shape check or throws-contract check
// against a generic `T` / `E` is **not viable today** — it would need
// (a) the exhaustiveness analyzer to treat generic-typed patterns as
// non-exhaustive, AND (b) pattern compilation to emit a runtime
// type-test against the substituted type at monomorphization. Both are
// real compiler / VM changes; absent those, the host-call typechecking
// must stay in Rust (as it is today) or use a different BAML construct
// (e.g. an explicit `reflect.Type.of<T>().matches(v)` builtin if added).
// ============================================================================

#[ignore = "documents a compiler gap: generic-typed patterns don't substitute T at runtime"]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn smoke_generic_pattern_in_match_substitutes_t() {
    // `match` failed E0063 (unreachable arm — analyzer treats `let x: T`
    // with a generic T as exhaustive). Try `if let` instead (refutable
    // pattern in condition position; the else branch is the implicit
    // mismatch case).
    let source = r#"
        function pick<T>(v: unknown, fallback: T) -> T {
            if let x: T = v {
                x
            } else {
                fallback
            }
        }

        // Positive: v is int(42), T=int → pattern binds, returns 42.
        function smoke_match_hit() -> int {
            return pick<int>(42, -1);
        }

        // Negative: v is string, T=int → pattern misses, returns fallback -1.
        function smoke_match_miss() -> int {
            return pick<int>("nope", -1);
        }
    "#;
    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );

    let hit = engine
        .call_function(
            "smoke_match_hit",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;
    assert!(
        matches!(hit, Ok(BexExternalValue::Int(42))),
        "match arm `let x: T` with T=int and v=Int(42) should bind + return 42; got {hit:?}"
    );

    let miss = engine
        .call_function(
            "smoke_match_miss",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;
    assert!(
        matches!(miss, Ok(BexExternalValue::Int(-1))),
        "match arm `let x: T` with T=int and v=String should NOT match → fallback -1; got {miss:?}"
    );
}

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
    /// Complete the call with a fatal `VmInternalError::BridgeFailure` —
    /// models a host-bridge wire-protocol bug (e.g. `is_error=1` with an
    /// empty payload, or an `is_error` outside `{0, 1}`). Must surface to
    /// the engine as `EngineError::VmInternalError` /
    /// `TracedVmInternalError` rather than an `UnhandledThrow`.
    BridgeFailure { message: String },
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

    // The engine sends a `BamlToHostCall` it already resolved against the
    // callable's declared params (omitted optionals dropped), with `args` in
    // declared order. Real bridges apply their calling convention (TS `$opts`,
    // Python kwargs); these tests invoke positionally, so we take every arg's
    // value in order. The callables exercised here have no optional params.
    let to_host_call = match BamlToHostCall::decode(bytes.as_slice()) {
        Ok(v) => v,
        Err(e) => {
            complete_with_test_error(call_id, "DecodeError", &format!("decode failure: {e}"));
            return;
        }
    };
    let items: Vec<BamlOutboundValue> = to_host_call
        .args
        .into_iter()
        .filter_map(|arg| arg.value)
        .collect();

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
        FakeReturn::BridgeFailure { message } => {
            sys_native::host_dispatch::complete_with_error(
                call_id,
                sys_types::OpError::new(
                    sys_types::SysOp::BamlHostCallHostValue,
                    sys_types::VmInternalError::BridgeFailure { message },
                ),
            );
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
    // Mirror what a real bridge SDK does when surfacing a native host
    // exception: build an `Instance` of `baml.errors.HostCallable`
    // carrying the host class identity + traceback in its fields, then
    // hand it to `host_dispatch::complete_with_throw`. The engine's
    // `materialize_host_throw` then runs the throws-contract check +
    // Value materialization + unwind injection.
    let mut fields = IndexMap::new();
    fields.insert(
        "message".to_string(),
        BexExternalValue::String(message.to_string().into()),
    );
    fields.insert(
        "class_name".to_string(),
        BexExternalValue::String(class_name.to_string().into()),
    );
    fields.insert(
        "language".to_string(),
        BexExternalValue::String("rust".to_string().into()),
    );
    fields.insert("traceback".to_string(), BexExternalValue::Null);
    // The class's `_handle $rust_type` slot is required by the engine's
    // structural check. Use a synthetic `HostValue(kind=Opaque)` handle
    // so the BAML→host decoder has *something* to round-trip — the
    // test doesn't rehydrate, so the key value is arbitrary.
    fields.insert(
        "_handle".to_string(),
        BexExternalValue::HostValue(HostValueArc::new(next_host_key(), HostValueKind::Opaque)),
    );
    sys_native::host_dispatch::complete_with_throw(
        call_id,
        BexExternalValue::Instance {
            class_name: "baml.errors.HostCallable".to_string(),
            type_args: vec![],
            fields,
        },
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
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
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

/// A callable that crosses a host boundary may itself be host-owned. APIs such
/// as the HTTP server retain a callable handle and later ask the engine to
/// invoke it as a fresh VM root, so that entry path must accept the same
/// `HostClosure` values that `CallIndirect` accepts inside BAML bytecode.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_callable_handle_can_be_invoked_as_engine_entry_point() {
    let source = r#"
        function retain_callable(f: (int) -> int throws never) -> (int) -> int throws never {
            return f;
        }
    "#;
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
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("engine construction"),
    );

    let retained = engine
        .call_function(
            "retain_callable",
            vec![BexExternalValue::HostValue(Arc::clone(&arc))],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            false,
        )
        .await
        .expect("retaining the host callable should succeed");
    let BexExternalValue::Handle(handle) = retained else {
        panic!("retained callable should cross the boundary as a handle");
    };

    let result = engine
        .call_callable(
            handle,
            vec![BexExternalValue::Int(41)],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("host callable handle should be invokable");
    assert_eq!(result, BexExternalValue::Int(42));
    drop(arc);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn returned_closure_required_arguments_bind_by_position_not_source_name() {
    let source = r#"
        function make_adder(offset: int) -> (value: int) -> int throws never {
            return (x: int) -> int { offset + x }
        }
    "#;
    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("engine construction"),
    );
    let returned = engine
        .call_function(
            "make_adder",
            vec![BexExternalValue::Int(10)],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            false,
        )
        .await
        .expect("returning the closure should succeed");
    let BexExternalValue::Handle(handle) = returned else {
        panic!("returned closure should cross the boundary as a handle");
    };

    let result = engine
        .call_callable_named(
            handle,
            indexmap::IndexMap::from([("value".to_string(), BexExternalValue::Int(5))]),
            indexmap::IndexMap::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("declared and implementation parameter names may differ");
    assert_eq!(result, BexExternalValue::Int(15));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_callable_arguments_preserve_closed_union_selected_arm_on_wire() {
    let source = r#"
        function round_trip(
            f: (int | string) -> int | string,
            value: int | string,
        ) -> int | string {
            f(value)
        }
    "#;
    let union_ty = RuntimeTy::union([RuntimeTy::int(), RuntimeTy::string()]);
    let arc = register_host_callable(|items| {
        let Some(baml_outbound_value::Value::UnionVariantValue(union)) =
            items.first().and_then(|value| value.value.as_ref())
        else {
            panic!("expected selected union envelope, got {:?}", items.first())
        };
        assert_eq!(union.value_option_name, "int");
        assert_eq!(union.selected_option_index, Some(0));
        assert!(matches!(
            union
                .value
                .as_deref()
                .and_then(|value| value.value.as_ref()),
            Some(baml_outbound_value::Value::IntValue(7))
        ));
        FakeReturn::Ok(BexExternalValue::union(
            BexExternalValue::String("seven".into()),
            [RuntimeTy::int(), RuntimeTy::string()],
            RuntimeTy::string(),
        ))
    });
    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );
    let result = engine
        .call_function(
            "round_trip",
            vec![
                BexExternalValue::HostValue(Arc::clone(&arc)),
                BexExternalValue::union(
                    BexExternalValue::Int(7),
                    [RuntimeTy::int(), RuntimeTy::string()],
                    RuntimeTy::int(),
                ),
            ],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("closed-union callback should succeed");
    let BexExternalValue::Union { value, metadata } = result else {
        panic!("expected selected union result")
    };
    assert_eq!(metadata.union_type, union_ty);
    assert_eq!(metadata.selected_option, RuntimeTy::string());
    assert!(matches!(*value, BexExternalValue::String(ref text) if text.as_str() == "seven"));
    drop(arc);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_callable_argument_selects_implemented_interface_arm_on_wire() {
    let source = r#"
        interface Failure {}
        class ProviderFailure {
            message string
        }
        implement Failure for ProviderFailure {}

        function invoke(f: (Failure | string) -> string) -> string {
            f(ProviderFailure { message: "boom" })
        }
    "#;
    let arc = register_host_callable(|items| {
        let Some(baml_outbound_value::Value::UnionVariantValue(union)) =
            items.first().and_then(|value| value.value.as_ref())
        else {
            panic!("expected selected interface-union envelope, got {items:?}")
        };
        assert_eq!(union.selected_option_index, Some(0));
        assert!(matches!(
            union
                .value
                .as_deref()
                .and_then(|value| value.value.as_ref()),
            Some(baml_outbound_value::Value::ClassValue(class))
                if class.name == "user.ProviderFailure"
        ));
        FakeReturn::Ok(BexExternalValue::String("handled".into()))
    });
    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "invoke",
            vec![BexExternalValue::HostValue(Arc::clone(&arc))],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("interface-union callback should succeed");

    assert!(matches!(
        result,
        BexExternalValue::String(ref value) if value.as_str() == "handled"
    ));
    drop(arc);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_callable_union_envelope_preserves_empty_container_arm_identity() {
    let source = r#"
        function round_trip(
            f: (int[] | string[]) -> int[] | string[],
            value: int[] | string[],
        ) -> int[] | string[] {
            f(value)
        }
    "#;
    let int_list = RuntimeTy::List(Box::new(RuntimeTy::int()), baml_type::TyAttr::default());
    let string_list = RuntimeTy::List(Box::new(RuntimeTy::string()), baml_type::TyAttr::default());
    let arc = register_host_callable({
        let int_list = int_list.clone();
        let string_list = string_list.clone();
        move |items| {
            let Some(baml_outbound_value::Value::UnionVariantValue(union)) =
                items.first().and_then(|value| value.value.as_ref())
            else {
                panic!(
                    "expected selected list-union envelope, got {:?}",
                    items.first()
                )
            };
            assert_eq!(union.value_option_name, "int[]");
            let Some(baml_outbound_value::Value::ListValue(list)) = union
                .value
                .as_deref()
                .and_then(|value| value.value.as_ref())
            else {
                panic!("expected list payload")
            };
            assert!(list.items.is_empty());
            FakeReturn::Ok(BexExternalValue::union(
                BexExternalValue::Array {
                    element_type: RuntimeTy::string(),
                    items: vec![],
                },
                [int_list.clone(), string_list.clone()],
                string_list.clone(),
            ))
        }
    });
    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );
    let result = engine
        .call_function(
            "round_trip",
            vec![
                BexExternalValue::HostValue(Arc::clone(&arc)),
                BexExternalValue::union(
                    BexExternalValue::Array {
                        element_type: RuntimeTy::int(),
                        items: vec![],
                    },
                    [int_list.clone(), string_list.clone()],
                    int_list,
                ),
            ],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("empty selected container callback should succeed");
    let BexExternalValue::Union { value, metadata } = result else {
        panic!("expected selected union result")
    };
    assert_eq!(metadata.selected_option, string_list);
    assert!(matches!(*value, BexExternalValue::Array { ref items, .. } if items.is_empty()));
    drop(arc);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_callable_optional_union_is_omitted_or_sent_with_selected_arm() {
    let source = r#"
        function invoke(f: (value?: int | string) -> int) -> int[] {
            [f(), f(value = "supplied")]
        }
    "#;
    let call_count = Arc::new(AtomicU64::new(0));
    let arc = register_host_callable({
        let call_count = Arc::clone(&call_count);
        move |items| {
            let call = call_count.fetch_add(1, Ordering::SeqCst);
            match call {
                0 => assert!(items.is_empty(), "omitted optional must not be sent"),
                1 => {
                    let Some(baml_outbound_value::Value::UnionVariantValue(union)) =
                        items.first().and_then(|value| value.value.as_ref())
                    else {
                        panic!("expected supplied optional union envelope")
                    };
                    assert_eq!(union.value_option_name, "string");
                    assert!(matches!(
                        union
                            .value
                            .as_deref()
                            .and_then(|value| value.value.as_ref()),
                        Some(baml_outbound_value::Value::StringValue(value))
                            if value == "supplied"
                    ));
                }
                _ => panic!("callback invoked more than twice"),
            }
            FakeReturn::Ok(BexExternalValue::Int(
                i64::try_from(call).expect("test callback count fits in i64"),
            ))
        }
    });
    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );
    let result = engine
        .call_function(
            "invoke",
            vec![BexExternalValue::HostValue(Arc::clone(&arc))],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("optional union callback should succeed");
    assert!(matches!(
        result,
        BexExternalValue::Array { ref items, .. }
            if matches!(items.as_slice(), [BexExternalValue::Int(0), BexExternalValue::Int(1)])
    ));
    drop(arc);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explicit_local_id_rejects_host_callable_with_catchable_invalid_argument() {
    let source = r#"
        function call_host_with_id(
            f: (int) -> int throws baml.errors.InvalidArgument,
            x: int,
        ) -> string {
            baml.json.to_string(f(x, $id = boundary.id())) catch (e) {
                baml.errors.InvalidArgument => "caught"
            }
        }
    "#;
    let arc = register_host_callable(|_items| FakeReturn::Ok(BexExternalValue::Int(999)));
    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("engine construction"),
    );
    let result = engine
        .call_function(
            "call_host_with_id",
            vec![
                BexExternalValue::HostValue(Arc::clone(&arc)),
                BexExternalValue::Int(1),
            ],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("host-callable rejection should be caught in BAML");
    assert_eq!(result, BexExternalValue::String("caught".into()));
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
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );

    let xs = BexExternalValue::Array {
        element_type: baml_type::RuntimeTy::int(),
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
//        args as `[handle, args_pack, ret_ty, throws_ty]` to match the
//        codegen-generated glue (`sys_ops/.../io_generated.rs` extracts `__arg0`
//        as the handle via `as_owned_but_very_slow`, `__arg1` as the args pack,
//        and `__arg2`/`type_arg_0` + `__arg3`/`type_arg_1` via
//        `as_baml_type_owned`). Nothing else pins this contract,
//        so a future codegen change to the type-arg operand position (or a swap
//        of any two operands) would silently break host calls. This test makes
//        the contract explicit and fails loudly if the operand order changes:
//
//   * `args[0]` (handle) — proven by the dispatch firing for the *correct*
//     registered key (the behaviour table lookup by `host_value_key` succeeds);
//     if `args[0]` were not the handle, `as_owned_but_very_slow` would not yield
//     a `HostValue` and the sys-op would fail with a `TypeError` before dispatch.
//   * `args[1]` (args pack) — the `[positional_array, optional_map]` pair the VM
//     builds; the dispatch decodes the `BamlToHostCall` args and asserts the
//     values are exactly the user args, in order: `[Int(7), Int(8)]`. If
//     `args[1]` carried the ret_ty `Object::Type` instead, the glue's
//     `BexExternalValue::Array` extraction would fail.
//   * `args[2]` (ret_ty) — proven to carry the declared return `RuntimeTy` (`int`):
//     the host returns the sum (an `Int`), which only passes return-type
//     validation if `type_arg_0` decoded to `int`. If `args[2]` carried the
//     args pack instead, `as_baml_type_owned` would fail and the call would
//     error before reaching the host.
//   * `args[3]` (throws_ty) — packed by codegen and consumed by the
//     engine's host-throw injection site (`lib.rs::execute_sys_op` →
//     `inject_sysop_throw`). The behaviour-level assertion lives in
//     [`host_callable_off_contract_throw_panics_as_host_contract_violation`]:
//     a recognizably-distinct `E` is declared and a mismatched host throw
//     becomes `baml.panics.HostContractViolation`.
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
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
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
// Wrong-return-type path: host returns String where int is declared.
//         This is a typed-contract violation — surfaces as a
//         `baml.panics.HostContractViolation`, not a catchable
//         `baml.errors.HostCallable`.
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_callable_wrong_return_type_panics_as_host_contract_violation() {
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
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
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
            BexExternalValue::Instance {
                class_name, fields, ..
            } => {
                assert_eq!(
                    class_name, "baml.panics.HostContractViolation",
                    "expected baml.panics.HostContractViolation instance, got {class_name}"
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
        other => panic!("expected UnhandledThrow(HostContractViolation), got {other:?}"),
    }
    drop(arc);
}

// ============================================================================
// Wrong-return-type when the callable *also* declares a concrete throws
//         contract. The engine builds a `HostContractViolation` panic for
//         the wrong return, then injects it through the same throw machinery
//         used by host throws. The contract-check pass must recognize the
//         panic as a panic and NOT re-validate it against `E` (which would
//         wrap it in a second `HostContractViolation` and overwrite the
//         original diagnostic). Mirrors the BAML rule that a fn's `throws E`
//         clause never includes panics.
// ============================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_callable_wrong_return_with_declared_throws_keeps_return_diagnostic() {
    let source = r#"
        class ParseError {
            detail string
        }
        function add_one(f: (int) -> int throws ParseError, x: int) -> int {
            return f(x);
        }
    "#;

    // Behaviour: return a string where an int is expected, on a callable
    // with a concrete `throws ParseError`. Pre-fix this would rewrite the
    // "expected int, got string" diagnostic into "host callable threw a
    // value of type `baml.panics.HostContractViolation` that is not in
    // its declared throws contract (`ParseError`)" — the int/string detail
    // would be lost.
    let arc = register_host_callable(|_items| {
        FakeReturn::Ok(BexExternalValue::String("not-an-int".to_string().into()))
    });

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
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
            BexExternalValue::Instance {
                class_name, fields, ..
            } => {
                assert_eq!(
                    class_name, "baml.panics.HostContractViolation",
                    "expected baml.panics.HostContractViolation, got {class_name}"
                );
                match fields.get("message") {
                    Some(BexExternalValue::String(m)) => {
                        assert!(
                            m.contains("string") && m.contains("int"),
                            "expected the int/string return-type diagnostic, got {m:?} \
                             (this means the panic was re-validated against `ParseError` \
                             and the original message was overwritten)"
                        );
                        assert!(
                            !m.contains("ParseError"),
                            "diagnostic should not mention the throws contract, got {m:?}"
                        );
                    }
                    other => panic!("expected a non-empty message field, got {other:?}"),
                }
            }
            other => panic!("expected Instance, got {other:?}"),
        },
        other => panic!("expected UnhandledThrow(HostContractViolation), got {other:?}"),
    }
    drop(arc);
}

/// Assert that an *uncaught* host-callable error surfaces as a structured
/// `root.errors.HostCallable` throw at the engine boundary.
///
/// Sysop errors now ride the VM's exception machinery (`inject_sysop_throw`
/// → `try_handle_external_exception`), so an in-BAML `try { … } catch (e: …)
/// { … }` catches them like any other throw — exercised by the
/// [`host_callable_throw_caught_in_baml`] test below. This helper covers
/// the complementary case: no in-BAML handler matched, so the throw
/// escapes all frames and lands on the host as `EngineError::UnhandledThrow`
/// carrying the structured instance.
fn assert_host_callable_throw(result: &Result<BexExternalValue, EngineError>) {
    match result {
        Err(EngineError::UnhandledThrow { value, .. }) => match value.as_ref() {
            BexExternalValue::Instance {
                class_name, fields, ..
            } => {
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

/// Assert that a host-callable contract violation surfaces as
/// `baml.panics.HostContractViolation` — used for wrong-return-type and
/// off-throws-contract scenarios. These are panics, not catchable errors:
/// they ride the same `EngineError::UnhandledThrow` envelope but carry a
/// `baml.panics.*` instance instead of `baml.errors.*`.
fn assert_host_contract_violation_panic(result: &Result<BexExternalValue, EngineError>) {
    match result {
        Err(EngineError::UnhandledThrow { value, .. }) => match value.as_ref() {
            BexExternalValue::Instance {
                class_name, fields, ..
            } => {
                assert_eq!(
                    class_name, "baml.panics.HostContractViolation",
                    "expected baml.panics.HostContractViolation instance, got {class_name}"
                );
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
        other => panic!("expected UnhandledThrow(HostContractViolation), got {other:?}"),
    }
}

// ============================================================================
// Class field-type mismatch: the declared return class has an `int`
//         field, but the host fills it with a string. The shared FFI-boundary
//         guard validates class-*name* identity; the engine-side schema-aware
//         check rejects the wrong field *type*. This is still a wrong-return-
//         type contract violation, so it panics with
//         `baml.panics.HostContractViolation`.
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_callable_wrong_class_field_type_panics_as_host_contract_violation() {
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
            type_args: vec![],
            fields,
        })
    });

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
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

    assert_host_contract_violation_panic(&result);
    drop(arc);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_callable_wrong_generic_class_field_type_panics_as_host_contract_violation() {
    let source = r#"
        class Box<T> {
            value T
        }
        function make_box(f: () -> Box<int>) -> Box<int> {
            return f();
        }
    "#;

    // Behaviour: return a Box whose generic `value` field is instantiated
    // as `int`, but the host fills it with a string. Like the non-generic
    // wrong-field-type case above, this is a typed-contract violation —
    // the engine's deep schema check rejects the string-in-int slot and
    // panics with `baml.panics.HostContractViolation`. (Earlier in the
    // branch this surfaced as a catchable `HostCallable`; the design
    // unifies wrong-return-type as a panic across the generic and non-
    // generic cases, so this test now matches the non-generic variant.)
    let arc = register_host_callable(|_items| {
        let mut fields = IndexMap::new();
        fields.insert(
            "value".to_string(),
            BexExternalValue::String("oops".to_string().into()),
        );
        FakeReturn::Ok(BexExternalValue::Instance {
            class_name: "Box".to_string(),
            type_args: vec![],
            fields,
        })
    });

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "make_box",
            vec![BexExternalValue::HostValue(Arc::clone(&arc))],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    assert_host_contract_violation_panic(&result);
    drop(arc);
}

// ============================================================================
// Bridge-wire-protocol bug: the host bridge synthesises
//         `VmInternalError::BridgeFailure` (e.g. an empty
//         `complete_host_call(is_error=1)` payload, or an `is_error` outside
//         `{0, 1}`). This is *infrastructure*, not a user contract violation:
//         it must surface as a fatal `EngineError::VmInternalError` /
//         `TracedVmInternalError`, NOT as an `UnhandledThrow` of
//         `baml.panics.HostContractViolation` (which would falsely accuse
//         the user's callable of returning the wrong shape).
//
//         The host SDKs (bridge_python / bridge_typescript) then render this
//         internal error as `baml.panics.SdkPanic` on their side, but that
//         translation is the bridge SDK's responsibility; the engine's
//         contract is only to surface it as an internal error.
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_callable_bridge_failure_surfaces_as_internal_error() {
    let source = r#"
        function call_cb(f: (int) -> int, x: int) -> int {
            return f(x);
        }
    "#;

    let arc = register_host_callable(|_items| FakeReturn::BridgeFailure {
        message: "synthetic wire-protocol fault".to_string(),
    });

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "call_cb",
            vec![
                BexExternalValue::HostValue(Arc::clone(&arc)),
                BexExternalValue::Int(1),
            ],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    match result {
        Err(
            EngineError::VmInternalError(sys_types::VmInternalError::BridgeFailure { message })
            | EngineError::TracedVmInternalError {
                source: sys_types::VmInternalError::BridgeFailure { message },
                ..
            },
        ) => {
            assert!(
                message.contains("synthetic wire-protocol fault"),
                "expected the bridge-supplied diagnostic to round-trip onto the internal \
                 error, got {message:?}"
            );
        }
        other => panic!(
            "expected EngineError::VmInternalError / TracedVmInternalError carrying \
             BridgeFailure; got {other:?}"
        ),
    }
    drop(arc);
}

// ============================================================================
// Off-contract throw: a callback declares `throws ParseError` but the host
//         throws a different class. The engine's throws-contract check
//         (`materialize_host_throw` against `type_arg_1 = E`) must reject this
//         mismatch and surface it as `baml.panics.HostContractViolation` —
//         NOT as a catchable `HostCallable` (that would silently admit a
//         throw the BAML signature said couldn't happen).
//
//         Pairs with `wrong_return_type_panics_as_host_contract_violation`:
//         that one covers the return-side contract (T), this covers the
//         throws-side contract (E). Together they pin both halves of the
//         host-callable typed-contract enforcement.
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_callable_off_contract_throw_panics_as_host_contract_violation() {
    let source = r#"
        class ParseError {
            detail string
        }
        function call_typed(f: (int) -> int throws ParseError, x: int) -> int {
            return f(x);
        }
    "#;

    // Behaviour: throw something that is NOT a `ParseError`. The fake
    // dispatch wraps every throw as a `baml.errors.HostCallable` Instance
    // (per `complete_with_test_error`), which decidedly does not subtype
    // the declared `throws ParseError` — so the engine's
    // `materialize_host_throw` contract check must reject and panic with
    // `HostContractViolation`.
    let arc = register_host_callable(|_items| FakeReturn::Err {
        class_name: "RuntimeError".to_string(),
        message: "boom".to_string(),
    });

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "call_typed",
            vec![
                BexExternalValue::HostValue(Arc::clone(&arc)),
                BexExternalValue::Int(1),
            ],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    assert_host_contract_violation_panic(&result);
    drop(arc);
}

// ============================================================================
// On-contract throw via interface membership: a callback declares
//         `throws HasMessage` (an interface) and the host throws a
//         `baml.errors.HostCallable`, which *implements* `HasMessage` (via a
//         standalone `implement HasMessage for baml.errors.HostCallable` block).
//         The throws-contract check must see the membership and treat the throw
//         as on-contract — surfacing it as a catchable `HostCallable`, NOT a
//         `HostContractViolation` panic.
//
//         Pins the fix that routes the contract check through the canonical,
//         program-aware type algebra (`baml_type::normalize::is_subtype` over the
//         VM as its `TypeContext`), which resolves interface membership. The prior
//         context-free `RuntimeTy::is_subtype_of` fork saw no membership —
//         `Class <: Interface` was simply `false` — and would have wrongly
//         rejected this throw as a contract violation.
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_callable_throw_implementing_interface_contract_is_on_contract() {
    // A method interface (not a field interface — those may only be implemented
    // in-body, E0126) so `HostCallable` can implement it out-of-body.
    let source = r#"
        interface Failure {
            function kind(self) -> string throws never
        }
        implement Failure for baml.errors.HostCallable {
            function kind(self) -> string throws never { "host" }
        }
        function call_typed(f: (int) -> int throws Failure, x: int) -> int {
            return f(x);
        }
    "#;

    // The fake dispatch materializes every throw as a `baml.errors.HostCallable`
    // instance (per `complete_with_test_error`). `HostCallable` implements the
    // declared `throws Failure` interface, so the throw is on-contract and must
    // propagate as a catchable value rather than a contract-violation panic.
    let arc = register_host_callable(|_items| FakeReturn::Err {
        class_name: "RuntimeError".to_string(),
        message: "boom".to_string(),
    });

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "call_typed",
            vec![
                BexExternalValue::HostValue(Arc::clone(&arc)),
                BexExternalValue::Int(1),
            ],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    assert_host_callable_throw(&result);
    drop(arc);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unhandled_throw_selects_implemented_interface_arm_in_throws_union() {
    let source = r#"
        interface Failure {
            function kind(self) -> string throws never
        }
        implement Failure for baml.errors.HostCallable {
            function kind(self) -> string throws never { "host" }
        }
        function call_typed(
            f: (int) -> int throws Failure,
            x: int,
        ) -> int throws Failure | baml.errors.UnknownError {
            return f(x);
        }
    "#;

    let arc = register_host_callable(|_items| FakeReturn::Err {
        class_name: "RuntimeError".to_string(),
        message: "boom".to_string(),
    });

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "call_typed",
            vec![
                BexExternalValue::HostValue(Arc::clone(&arc)),
                BexExternalValue::Int(1),
            ],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    match result {
        Err(EngineError::UnhandledThrow { value, .. }) => match value.as_ref() {
            BexExternalValue::Union { value, metadata } => {
                assert!(
                    matches!(
                        &metadata.selected_option,
                        RuntimeTy::Interface(name, _, _, _) if name.to_string() == "user.Failure"
                    ),
                    "expected the Failure interface arm, got {:?}",
                    metadata.selected_option,
                );
                assert!(
                    matches!(
                        value.as_ref(),
                        BexExternalValue::Instance { class_name, .. }
                            if class_name == "baml.errors.HostCallable"
                    ),
                    "expected the concrete HostCallable throw, got {value:?}",
                );
            }
            other => panic!("expected union-wrapped HostCallable throw, got {other:?}"),
        },
        other => panic!("expected UnhandledThrow, got {other:?}"),
    }
    drop(arc);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn root_return_selects_implemented_interface_arm_in_union() {
    let source = r#"
        interface Failure {}
        class ProviderFailure {
            message string
        }
        implement Failure for ProviderFailure {}

        function provider_failure() -> Failure | string {
            ProviderFailure { message: "boom" }
        }
    "#;
    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "provider_failure",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("interface-union return should succeed");

    let BexExternalValue::Union { value, metadata } = result else {
        panic!("expected selected interface-union return")
    };
    assert!(matches!(
        &metadata.selected_option,
        RuntimeTy::Interface(name, _, _, _) if name.to_string() == "user.Failure"
    ));
    assert!(matches!(
        value.as_ref(),
        BexExternalValue::Instance { class_name, .. }
            if class_name == "user.ProviderFailure"
    ));
}

// ============================================================================
// Undeclared callback ⇒ `throws unknown` contract accepts a native throw as
//         opaque. The FFI entry boundary normalizes the synthesized effect
//         param (post-MIR `RuntimeTy::Void`) to `RuntimeTy::BuiltinUnknown` so the contract
//         check at `materialize_host_throw` treats any thrown value as
//         on-contract — including the opaque `baml.errors.HostCallable`
//         Instance the bridge synthesizes for a native host exception. The
//         throw propagates as a regular catchable value through the call
//         graph; with no in-BAML `catch`, it surfaces at the engine boundary
//         as an `UnhandledThrow` carrying the original `HostCallable`.
//
//         This pins the "throws unknown" fallback: a host-provided callable
//         whose error contract is undeclared must NOT be admitted by a
//         concrete-throws check (that's the off-contract case above) and
//         must NOT be rejected by an over-strict `RuntimeTy::Void` validator (the
//         pre-D1 erasure path). Both failure modes are guarded against.
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_callable_undeclared_callback_accepts_native_throw_as_opaque() {
    let source = r#"
        function call_untyped(f: (int) -> int, x: int) -> int {
            return f(x);
        }
    "#;

    // Behaviour: throw a generic native exception. With no declared throws
    // on `f`, the synthesized effect param normalizes to `unknown` at the
    // FFI boundary — `validate_host_return` accepts any value against
    // `BuiltinUnknown`, so the throw propagates as the catchable
    // `HostCallable` instance the fake dispatch builds.
    let arc = register_host_callable(|_items| FakeReturn::Err {
        class_name: "ValueError".to_string(),
        message: "native exception from host".to_string(),
    });

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "call_untyped",
            vec![
                BexExternalValue::HostValue(Arc::clone(&arc)),
                BexExternalValue::Int(1),
            ],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    // Assert the throw came through catchable (i.e. an `UnhandledThrow` of
    // `HostCallable`, NOT a `HostContractViolation` panic). This is the
    // critical distinction from the off-contract test above.
    assert_host_callable_throw(&result);
    drop(arc);
}

// ============================================================================
// In-BAML catch of a host throw: a callback declares `throws HostCallable`
//         and the surrounding BAML function wraps `f(x)` in a `catch` that
//         returns a recovery string. The host raises; the VM's exception
//         unwinder runs the catch handler, and the BAML function returns the
//         recovery value instead of propagating the throw to the host.
//
//         This was the previously-documented limitation (`call_with_throwing`
//         SDK xfail): a sysop throw bypassed the VM unwinder, so an in-BAML
//         `catch` couldn't match. The engine now injects sysop throws into
//         the same unwinder a `throw` opcode uses, so host throws are caught
//         like any other throw.
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_callable_throw_caught_in_baml() {
    let source = r#"
        function call_and_catch(
            f: (int) -> string throws baml.errors.HostCallable,
            x: int,
        ) -> string {
            f(x) catch (e) {
                _ => "caught:" + e.class_name
            }
        }
    "#;

    // The host's callable raises a generic `RuntimeError`. Because the
    // declared contract is `HostCallable`, the throw is admitted and
    // delivered as a catchable HostCallable; the in-BAML catch handles it.
    let arc = register_host_callable(|_items| FakeReturn::Err {
        class_name: "RuntimeError".to_string(),
        message: "boom".to_string(),
    });

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "call_and_catch",
            vec![
                BexExternalValue::HostValue(Arc::clone(&arc)),
                BexExternalValue::Int(1),
            ],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    // The catch ran; the function returned the recovery string.
    match &result {
        Ok(BexExternalValue::String(s)) => {
            assert_eq!(
                &**s, "caught:RuntimeError",
                "the in-BAML catch should run with e.class_name = the host class"
            );
        }
        other => panic!("expected the catch's recovery string, got {other:?}"),
    }
    drop(arc);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_callable_throw_normalized_as_unknown_error_preserves_context() {
    let source = r#"
        function invoke_host(
            f: (int) -> string throws baml.errors.HostCallable,
            x: int,
        ) -> string {
            f(x)
        }

        function normalize_host_throw(
            f: (int) -> string throws baml.errors.HostCallable,
            x: int,
        ) -> string throws baml.errors.UnknownError {
            invoke_host(f, x) catch_all (error) {
                _ => throw baml.errors.UnknownError.with_message<never>(
                    error,
                    "host callback failed",
                ),
            }
        }
    "#;

    let arc = register_host_callable(|_items| FakeReturn::Err {
        class_name: "RuntimeError".to_string(),
        message: "boom".to_string(),
    });

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "normalize_host_throw",
            vec![
                BexExternalValue::HostValue(Arc::clone(&arc)),
                BexExternalValue::Int(1),
            ],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    match result {
        Err(EngineError::UnhandledThrow { value, trace }) => {
            let BexExternalValue::Instance {
                class_name, fields, ..
            } = value.as_ref()
            else {
                panic!("expected UnknownError instance, got {value:?}");
            };
            assert_eq!(class_name, "baml.errors.UnknownError");
            assert!(matches!(
                fields.get("data"),
                Some(BexExternalValue::Instance { class_name, .. })
                    if class_name == "baml.errors.HostCallable"
            ));
            assert!(matches!(
                fields.get("message"),
                Some(BexExternalValue::Array { items, .. })
                    if matches!(items.as_slice(), [BexExternalValue::String(message)] if &**message == "host callback failed")
            ));
            assert!(
                trace
                    .iter()
                    .any(|frame| frame.function_name.ends_with("invoke_host")),
                "expected the original host-call frame in {trace:?}"
            );
        }
        other => panic!("expected UnhandledThrow(UnknownError), got {other:?}"),
    }
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
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
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
//         the strict enum-identity check (shared FFI-boundary guard) and —
//         like all wrong-return-type cases — surfaces as a
//         `baml.panics.HostContractViolation`, not a catchable error.
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_callable_wrong_enum_identity_panics_as_host_contract_violation() {
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
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
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

    assert_host_contract_violation_panic(&result);
    drop(arc);
}

// ============================================================================
// Function-typed return: the callback's declared return type is itself a
// callable (`() -> int`) and the host returns a second `HostValue`. The engine
// binds that result to a `HostClosure`, which can be invoked immediately.
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_callable_returning_a_callable_can_be_invoked() {
    let source = r#"
        function call_returns_callable(f: (int) -> (() -> int throws never), x: int) -> int {
            return f(x)();
        }
    "#;

    // The callback returns *another* host callable (a function-typed value).
    let returned = register_host_callable(|_items| FakeReturn::Ok(BexExternalValue::Int(99)));
    let returned_for_cb = Arc::clone(&returned);
    let arc = register_host_callable(move |_items| {
        FakeReturn::Ok(BexExternalValue::HostValue(Arc::clone(&returned_for_cb)))
    });

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
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

    assert!(
        matches!(result, Ok(BexExternalValue::Int(99))),
        "returned host callable should remain callable, got {result:?}"
    );
    drop(arc);
    drop(returned);
}

// ============================================================================
// A moving GC during the host-call await must not invalidate return-type
//         validation. The engine captures the declared return type as an owned
//         `RuntimeTy` before the await; if it instead re-read the raw `args[2]` heap
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
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
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
    // Wrong-return-type after a GC-relocated retty must still be caught by
    // the pre-captured owned `RuntimeTy`; the visible effect is the same as any
    // other wrong-return-type violation — `HostContractViolation` panic.
    assert_host_contract_violation_panic(&result);

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
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
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
//         call setup. A generic parameter's type variables erase to `RuntimeTy::Void`
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
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "apply_generic",
            vec![
                BexExternalValue::HostValue(Arc::clone(&arc)),
                BexExternalValue::Int(1),
            ],
            // No `_types=` bindings: `T` is left unbound. The generic return
            // can't be validated, so the call is rejected before the host
            // callable is bound (full-binding enforcement, lib.rs).
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    // Rejection: an unbound generic (its erased return can't be validated)
    // fails up front.
    assert!(
        matches!(result, Err(EngineError::TypeMismatch { .. })),
        "a generic-return host callable must be rejected at bind, got {result:?}"
    );
    drop(arc);
}

// ============================================================================
// Regression: a spawned child that AWAITS a future which settles to an
// `InternalError` must not leak its own future as `Pending` forever.
// ============================================================================

// Root cause (engine-shutdown wedge, reproduced from the live `baml test`
// sweep): when a spawned awaiter is PARKED on a future and that future settles
// to `InternalError`, the engine's `Await`/`AwaitAny` arm resolves the
// awaiter's `SetOnce` wait with an `Err(EngineError)` and propagates it via
// `AwaitOutcome::Done(r) => r?`. That bubbles the error out of
// `run_thread_event_loop` WITHOUT any `settle_child_*` running, so
// `spawn_thread_inner` used to only *log* it — leaving the awaiter's heap
// `Future` `Pending`. Its `ready` wake never fires, so ITS awaiter (and,
// transitively, the root's B-650 end-of-run wait) parks forever: the VM did all
// its work but the engine never observes quiescence and the process wedges.
//
// The live trigger was a VM `TracedVmInternalError` surfaced on live data (its
// `Display` renders with a Python-style `Traceback (most recent call last):`
// header — BAML's own internal-error formatter, not an actual Python
// exception). Here we seed the SAME leak deterministically and offline with a
// host-bridge fault (`FakeReturn::BridgeFailure`), which the engine surfaces as
// an uncatchable `EngineError::VmInternalError { BridgeFailure }` — a permanent
// host surface, so this regression test does not rot when unrelated language
// features change (unlike a type-soundness-hole seed; see Linear B-797).
//
// The 50ms sleep forces the "awaiter parked first" ordering the leak needs.
// The load-bearing assertion is COMPLETION, not a value: before the fix
// `call_function` never resolves (the wedge), so the test hangs and the
// `timeout` fires.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn spawned_awaiter_of_internal_error_future_does_not_wedge() {
    let source = r#"
        function main(boom: () -> string) -> string {
            // `f` yields (so its awaiter parks) then hits an uncatchable
            // host-bridge internal error.
            let f = spawn {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(50n));
                boom()
            };
            // `g` is a SPAWN that awaits `f`: its `r?`-propagated engine error
            // is what used to leak. Nesting under a spawn is essential — a plain
            // root `await` surfaces the error fine.
            let g = spawn { await f };
            await g
        }
    "#;

    // The host callable faults with a fatal BridgeFailure → uncatchable
    // `EngineError::VmInternalError` in whichever thread invokes it (here, `f`).
    let arc = register_host_callable(|_items| FakeReturn::BridgeFailure {
        message: "boom".to_string(),
    });

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );

    let call = engine.call_function(
        "main",
        vec![BexExternalValue::HostValue(Arc::clone(&arc))],
        FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        true,
    );
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(10), call).await;

    // Load-bearing: the engine future RESOLVED (no wedge).
    let result =
        outcome.expect("engine wedged: call_function never resolved (the spawn-leak regressed)");
    // With the fix the uncatchable internal error propagates deterministically
    // up the await chain, so `main` returns an internal error rather than hanging.
    assert!(
        matches!(
            result,
            Err(
                EngineError::VmInternalError(sys_types::VmInternalError::BridgeFailure { .. })
                    | EngineError::TracedVmInternalError {
                        source: sys_types::VmInternalError::BridgeFailure { .. },
                        ..
                    }
            )
        ),
        "expected the `BridgeFailure` internal error to surface after the fix, got {result:?}"
    );
    drop(arc);
}

/// The scenario-15 "concurrent guards" topology: children spawned inside a
/// closure (capturing a linked `CancelToken`), joined with `baml.future.all`
/// under a spawned parent — and ONE child hitting an uncatchable engine error
/// while its siblings are parked. Before the spawn-leak fix this parked at
/// 0% CPU forever (the guardrail deadlock documented in
/// `ns_ai_scenarios/15_guardrails`); with it, the error propagates and the
/// call resolves. Kept alongside the minimal nested-spawn repro because the
/// closure + `all()` + cancel-token topology exercises the settle path
/// through `future.all`'s cancel-the-rest arm as well.
///
/// NOTE vs #3959's verbatim form: the closure's typed effects are caught
/// inside the spawn bodies and the futures annotated `never` — the original
/// `Future<string, null>` claim is now rejected by (intentionally) stricter
/// effect inference, and the host-callable `callback` effect is not nameable
/// in user source while futures are invariant in the error parameter. The
/// bridge fault this test pins is an UNCATCHABLE engine error, which strikes
/// regardless of the catches. The `.map`-spawned shape itself also exercises
/// the closure-signature deduction fix (an annotated lambda argument commits
/// its written signature into the callee's expected function type), without
/// which the `.map` call's `U` stays unsolved during the walk and the body
/// miscompiles.
#[tokio::test]
async fn spawn_in_map_closure_with_erroring_child_does_not_wedge() {
    let source = r#"
        function main(boom: () -> string) -> string {
            let parent = spawn {
                let tok = baml.spawn.CancelToken.new();
                let items = [1, 2, 3];
                let futures = items.map((n: int) -> baml.future.Future<string, never> {
                    spawn with baml.spawn.options(cancel = tok) {
                        if (n == 2) {
                            // parks the awaiter first, then faults the bridge
                            baml.sys.sleep(baml.time.Duration.from_milliseconds(50n)) catch_all (e) {
                                _ => null
                            };
                            boom() catch_all (e) {
                                _ => "boom-absorbed"
                            }
                        } else {
                            baml.sys.sleep(baml.time.Duration.from_milliseconds(150n)) catch_all (e) {
                                _ => null
                            };
                            "guard-" + n.to_string()
                        }
                    }
                });
                let all = (await baml.future.all(futures)) catch_all (e) {
                    _ => ["all-failed"]
                };
                tok.cancel();
                all.join(",")
            };
            await parent
        }
    "#;

    let arc = register_host_callable(|_items| FakeReturn::BridgeFailure {
        message: "guard boom".to_string(),
    });

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );

    let call = engine.call_function(
        "main",
        vec![BexExternalValue::HostValue(Arc::clone(&arc))],
        FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        true,
    );
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(10), call).await;

    // Load-bearing assertion: the engine RESOLVED (no 0%-CPU park). The exact
    // shape of the result is secondary — an uncatchable bridge fault may
    // surface as an engine error or be absorbed by the catch_all depending on
    // scheduling; the deadlock is what this test pins.
    let _result = outcome.expect(
        "engine wedged: spawn-in-map guard topology never resolved (the spawn-leak regressed)",
    );
}

/// Shutdown-deadline regression: a fire-and-forget spawn that outlives its
/// call (the mock-server-leak shape — a test finishes but its background task
/// never settles) must not park shutdown forever. With a grace of 500ms,
/// `shutdown_with_deadline` cancels the straggler, force-settles it if it
/// ignores the token, reports its spawn provenance, and returns. Without the
/// deadline this waited the full sleep (60s) — the "Waiting for N remaining
/// BAML futures" forever-loop, in miniature.
#[tokio::test]
async fn shutdown_deadline_abandons_leaked_spawn_and_reports_origin() {
    let source = r#"
        function main() -> string {
            let _leak = spawn {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(60000n)) catch_all (e) {
                    _ => null
                };
                "never observed"
            };
            "done"
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("main should return");
    assert_eq!(result, BexExternalValue::String("done".into()));

    let started = std::time::Instant::now();
    let leaks: Arc<std::sync::Mutex<Vec<bex_engine::LeakedFuture>>> = Arc::default();
    let leaks_out = Arc::clone(&leaks);
    let shutdown = engine.shutdown_with_deadline(
        Some(std::time::Duration::from_millis(500)),
        |_count| {},
        move |observed| {
            leaks_out
                .lock()
                .expect("leak sink lock")
                .extend(observed.iter().cloned());
        },
    );
    tokio::time::timeout(std::time::Duration::from_secs(20), shutdown)
        .await
        .expect("shutdown wedged: the deadline did not bound the leaked-spawn wait");
    // Well under the leaked sleep's 60s: the deadline (0.5s) plus the bounded
    // cooperative-settle window, not the sleep, decides the wall clock.
    assert!(
        started.elapsed() < std::time::Duration::from_secs(15),
        "shutdown took {:?}",
        started.elapsed()
    );

    let leaks = leaks.lock().expect("leak sink lock");
    assert_eq!(leaks.len(), 1, "expected exactly one leaked future");
    assert_eq!(
        leaks[0].origin.as_ref(),
        "user.main",
        "leak should be attributed to the spawning function"
    );
}
