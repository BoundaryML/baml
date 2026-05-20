//! End-to-end tests for Phase 5 of the external-function-handles BEP: a
//! host-language callable is bound to an `Object::HostClosure` at the FFI
//! boundary and dispatched through `SysOp::BamlHostCallHostValue` when BAML
//! code invokes it.
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

use bex_engine::{BexEngine, BexExternalValue, EngineError, FunctionCallContextBuilder};
use bex_resource_types::{HostValueArc, HostValueKind};
use bridge_ctypes::baml_core::cffi::{BamlOutboundValue, baml_outbound_value};
use common::compile_for_engine;
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
}

type Behaviour = Box<dyn Fn(Vec<BamlOutboundValue>) -> FakeReturn + Send + Sync>;

static BEHAVIOUR_TABLE: OnceLock<Mutex<HashMap<u64, Behaviour>>> = OnceLock::new();
static NEXT_KEY: AtomicU64 = AtomicU64::new(1);
static DISPATCH_REGISTERED: OnceLock<()> = OnceLock::new();

fn table() -> &'static Mutex<HashMap<u64, Behaviour>> {
    BEHAVIOUR_TABLE.get_or_init(|| Mutex::new(HashMap::new()))
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

    let behaviour = table().lock().unwrap().remove(&host_value_key);
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
    table().lock().unwrap().insert(key, Box::new(behaviour));
    HostValueArc::new(key, HostValueKind::Callable)
}

// ============================================================================
// 5.5.a — Success path: f(41) returns 42
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
// 5.5.b — Wrong-return-type path: host returns String where int is declared,
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
        FakeReturn::Ok(BexExternalValue::String("not-an-int".to_string()))
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
            BexExternalValue::Instance { class_name, .. } => {
                assert_eq!(
                    class_name, "baml.errors.HostCallable",
                    "expected baml.errors.HostCallable instance, got {class_name}"
                );
            }
            other => panic!("expected Instance, got {other:?}"),
        },
        other => panic!("expected UnhandledThrow(HostCallable), got {other:?}"),
    }
    drop(arc);
}
