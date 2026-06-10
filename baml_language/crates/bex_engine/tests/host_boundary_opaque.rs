//! Opaque host-only values bound to generic positions (bridge generics).
//!
//! A host value with no BAML representation crosses inbound as
//! `BexExternalValue::HostValue(arc, kind=Opaque)`. At an `unknown` position
//! (which is what an unbound/erased TypeVar is at the boundary) the engine
//! seals it as `Object::RustData` and the outbound path re-encodes the same
//! `(key, kind)` handle, so the originating bridge can rehydrate the
//! original host object. Inside BAML, `==`/`!=` are host-object identity;
//! ordering comparisons are errors; typed positions reject the value.

mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue, EngineError, FunctionCallContextBuilder};
use bex_external_types::{HostValueArc, HostValueKind};
use common::compile_for_engine;
use sys_native::SysOpsExt;

fn make_engine(source: &str) -> Arc<BexEngine> {
    let snapshot = compile_for_engine(source);
    Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .expect("Failed to create engine"),
    )
}

async fn call(
    engine: &Arc<BexEngine>,
    name: &str,
    args: Vec<BexExternalValue>,
) -> Result<BexExternalValue, EngineError> {
    engine
        .call_function(
            name,
            args,
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
}

fn opaque(key: u64) -> BexExternalValue {
    BexExternalValue::HostValue(HostValueArc::intern(key, HostValueKind::Opaque))
}

const IDENTITY: &str = r#"
    function identity<T>(x: T) -> T { x }
"#;

/// G16/H1: a host-only value bound to a bare `T` round-trips as the same
/// `(key, kind)` handle.
#[tokio::test]
async fn opaque_identity_roundtrip() {
    let engine = make_engine(IDENTITY);
    let out = call(&engine, "identity", vec![opaque(9001)]).await.unwrap();
    match out {
        BexExternalValue::HostValue(arc) => {
            assert_eq!(arc.key, 9001);
            assert_eq!(arc.kind, HostValueKind::Opaque);
        }
        other => panic!("expected HostValue back, got {other:?}"),
    }
}

/// H3: a host-only value stored in a baml-known generic class field survives
/// construction in BAML and comes back out inside the instance payload.
const WRAP: &str = r#"
    class Wrapper<T> { value T }
    function wrap<T>(x: T) -> Wrapper<T> { Wrapper { value: x } }
"#;

#[tokio::test]
async fn opaque_inside_generic_class_field() {
    let engine = make_engine(WRAP);
    let out = call(&engine, "wrap", vec![opaque(9002)]).await.unwrap();
    match out {
        BexExternalValue::Instance { class_name, fields } => {
            assert!(class_name.contains("Wrapper"), "got class {class_name}");
            match fields.get("value") {
                Some(BexExternalValue::HostValue(arc)) => {
                    assert_eq!(arc.key, 9002);
                    assert_eq!(arc.kind, HostValueKind::Opaque);
                }
                other => panic!("expected HostValue field, got {other:?}"),
            }
        }
        other => panic!("expected Instance, got {other:?}"),
    }
}

/// H2: host-only values as elements of `T[]`.
const FIRST: &str = r#"
    function first<T>(items: T[]) -> T { items[0] }
"#;

#[tokio::test]
async fn opaque_in_list_element() {
    let engine = make_engine(FIRST);
    let unknown = baml_type::Ty::BuiltinUnknown {
        attr: baml_type::TyAttr::default(),
    };
    let out = call(
        &engine,
        "first",
        vec![BexExternalValue::Array {
            element_type: unknown,
            items: vec![opaque(9003), opaque(9004)],
        }],
    )
    .await
    .unwrap();
    match out {
        BexExternalValue::HostValue(arc) => assert_eq!(arc.key, 9003),
        other => panic!("expected HostValue, got {other:?}"),
    }
}

/// H8: `==` on host-only values is host-object identity (same registry key),
/// `!=` is its negation.
const EQ: &str = r#"
    function eq<T>(a: T, b: T) -> bool { a == b }
"#;

#[tokio::test]
async fn opaque_equality_same_key_is_true() {
    let engine = make_engine(EQ);
    let out = call(&engine, "eq", vec![opaque(9005), opaque(9005)])
        .await
        .unwrap();
    assert_eq!(out, BexExternalValue::Bool(true));
}

#[tokio::test]
async fn opaque_equality_different_keys_is_false() {
    let engine = make_engine(EQ);
    let out = call(&engine, "eq", vec![opaque(9006), opaque(9007)])
        .await
        .unwrap();
    assert_eq!(out, BexExternalValue::Bool(false));
}

/// H8: ordering comparisons on sealed host values are errors, not silent
/// false.
const LT: &str = r#"
    function lt<T>(a: T, b: T) -> bool { a < b }
"#;

#[tokio::test]
async fn opaque_ordering_is_an_error() {
    let engine = make_engine(LT);
    let result = call(&engine, "lt", vec![opaque(9008), opaque(9009)]).await;
    assert!(
        result.is_err(),
        "`<` on opaque host values should error, got {result:?}"
    );
}

/// A typed (non-generic) position rejects an opaque host value: host-only
/// values bind to `unknown` positions only.
#[tokio::test]
async fn opaque_rejected_at_concrete_param() {
    let engine = make_engine("function wants_string(x: string) -> string { x }");
    let result = call(&engine, "wants_string", vec![opaque(9010)]).await;
    assert!(
        result.is_err(),
        "opaque host value at a `string` param should be rejected, got {result:?}"
    );
}

/// G9/H4: a union with a concrete arm and a `T` arm routes a host-only value
/// to the (erased) `T` arm — it can never match the concrete arm.
const UNION_PARAM: &str = r#"
    function tag<T>(x: T | string) -> string {
        if (x is string) { "string" } else { "host" }
    }
"#;

#[tokio::test]
async fn opaque_routes_to_typevar_arm_in_union() {
    let engine = make_engine(UNION_PARAM);
    let out = call(&engine, "tag", vec![opaque(9011)]).await.unwrap();
    assert_eq!(out, BexExternalValue::String("host".into()));
    let out = call(
        &engine,
        "tag",
        vec![BexExternalValue::String("s".into())],
    )
    .await
    .unwrap();
    assert_eq!(out, BexExternalValue::String("string".into()));
}
