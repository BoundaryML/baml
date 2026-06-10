//! Host-boundary semantics for generic functions (bridge-generics design).
//!
//! When the host calls a generic BAML function through `call_function`, the
//! declared parameter/return types may contain `Ty::TypeVar` leaves. The
//! boundary contract under test:
//!
//! 1. Explicit type args from the host (`FunctionCallContext::type_args`)
//!    substitute into param/return/throws types before coercion/validation,
//!    and seed the entry frame so `reflect.type_of<T>()` works.
//! 2. Any TypeVar left unbound is erased to `BuiltinUnknown` at the boundary:
//!    inbound coercion accepts any value, and return validation treats the
//!    position as "accept anything" (including as a union arm).
//! 3. Values typed by an unbound TypeVar round-trip unchanged.

mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue, EngineError, FunctionCallContextBuilder};
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

async fn call_with_type_args(
    engine: &Arc<BexEngine>,
    name: &str,
    args: Vec<BexExternalValue>,
    type_args: Vec<baml_type::Ty>,
) -> Result<BexExternalValue, EngineError> {
    engine
        .call_function(
            name,
            args,
            FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_type_args(type_args)
                .build(),
            true,
        )
        .await
}


/// Build a `Ty::Class` for a local user class by short name (mirrors what a
/// `$types` wire resolver will do engine-side).
fn class_ty(_engine: &Arc<BexEngine>, name: &str) -> baml_type::Ty {
    baml_type::Ty::Class(
        baml_type::QualifiedTypeName::local(baml_type::Name::new(name)),
        vec![],
        baml_type::TyAttr::default(),
    )
}

// ─────────────────────────────────────────────────────────────────────────
// G1: bare `T` parameter / return — identity roundtrip
// ─────────────────────────────────────────────────────────────────────────

const IDENTITY: &str = r#"
    function identity<T>(x: T) -> T { x }
"#;

#[tokio::test]
async fn identity_int_roundtrip() {
    let engine = make_engine(IDENTITY);
    let out = call(&engine, "identity", vec![BexExternalValue::Int(5)])
        .await
        .unwrap();
    assert_eq!(out, BexExternalValue::Int(5));
}

#[tokio::test]
async fn identity_string_roundtrip() {
    let engine = make_engine(IDENTITY);
    let out = call(
        &engine,
        "identity",
        vec![BexExternalValue::String("hi".into())],
    )
    .await
    .unwrap();
    assert_eq!(out, BexExternalValue::String("hi".into()));
}

#[tokio::test]
async fn identity_list_roundtrip() {
    let engine = make_engine(IDENTITY);
    let unknown = baml_type::Ty::BuiltinUnknown {
        attr: baml_type::TyAttr::default(),
    };
    let out = call(
        &engine,
        "identity",
        vec![BexExternalValue::Array {
            element_type: unknown,
            items: vec![BexExternalValue::Int(1), BexExternalValue::Int(2)],
        }],
    )
    .await
    .unwrap();
    match out {
        BexExternalValue::Array { items, .. } => {
            assert_eq!(
                items,
                vec![BexExternalValue::Int(1), BexExternalValue::Int(2)]
            );
        }
        other => panic!("expected Array, got {other:?}"),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// 22f "case B": TypeVar as a union arm in the return type. The classic
// failure: `T | Marker` with T unbound rejects every concrete payload.
// ─────────────────────────────────────────────────────────────────────────

const UNION_RETURN: &str = r#"
    class Marker { tag string }
    function pick<T>(x: T) -> T | Marker { x }
"#;

#[tokio::test]
async fn union_return_with_unbound_typevar_arm_accepts_value() {
    let engine = make_engine(UNION_RETURN);
    let out = call(
        &engine,
        "pick",
        vec![BexExternalValue::String("hello".into())],
    )
    .await
    .unwrap();
    // The union wrapper may or may not be present depending on the
    // outbound encoding; accept either shape but require the payload.
    let payload = match out {
        BexExternalValue::Union { value, .. } => *value,
        other => other,
    };
    assert_eq!(payload, BexExternalValue::String("hello".into()));
}

// ─────────────────────────────────────────────────────────────────────────
// G23-analog at the engine level: a method on a generic class, invoked the
// way host SDKs invoke it (FQN with `self` as arg0). Return type mentions
// the class-level TypeVar inside a union.
// ─────────────────────────────────────────────────────────────────────────

const GENERIC_METHOD: &str = r#"
    class Marker { tag string }
    class Holder<T> {
        value T
        function get(self) -> T { self.value }
        function get_or_marker(self) -> T | Marker { self.value }
    }
    function make_holder(text: string) -> Holder<string> {
        Holder<string> { value: text }
    }
"#;

#[tokio::test]
async fn generic_method_plain_typevar_return() {
    let engine = make_engine(GENERIC_METHOD);
    let holder = call(
        &engine,
        "make_holder",
        vec![BexExternalValue::String("hello".into())],
    )
    .await
    .unwrap();
    let out = call(&engine, "Holder.get", vec![holder]).await.unwrap();
    assert_eq!(out, BexExternalValue::String("hello".into()));
}

#[tokio::test]
async fn generic_method_union_typevar_return() {
    let engine = make_engine(GENERIC_METHOD);
    let holder = call(
        &engine,
        "make_holder",
        vec![BexExternalValue::String("hello".into())],
    )
    .await
    .unwrap();
    let out = call(&engine, "Holder.get_or_marker", vec![holder])
        .await
        .unwrap();
    let payload = match out {
        BexExternalValue::Union { value, .. } => *value,
        other => other,
    };
    assert_eq!(payload, BexExternalValue::String("hello".into()));
}

// ─────────────────────────────────────────────────────────────────────────
// G12: generic class parameter bound by the function's TypeVar.
// Host sends an untagged map for `Wrapper<T>`.
// ─────────────────────────────────────────────────────────────────────────

const WRAP_UNWRAP: &str = r#"
    class Wrapper<T> { value T }
    function unwrap<T>(w: Wrapper<T>) -> T { w.value }
    function wrap<T>(x: T) -> Wrapper<T> { Wrapper { value: x } }
"#;

#[tokio::test]
async fn unwrap_generic_class_param() {
    let engine = make_engine(WRAP_UNWRAP);
    let unknown = baml_type::Ty::BuiltinUnknown {
        attr: baml_type::TyAttr::default(),
    };
    let out = call(
        &engine,
        "unwrap",
        vec![BexExternalValue::Map {
            key_type: baml_type::Ty::String {
                attr: baml_type::TyAttr::default(),
            },
            value_type: unknown,
            entries: [("value".to_string(), BexExternalValue::Int(7))]
                .into_iter()
                .collect(),
        }],
    )
    .await
    .unwrap();
    assert_eq!(out, BexExternalValue::Int(7));
}

#[tokio::test]
async fn wrap_returns_generic_class() {
    let engine = make_engine(WRAP_UNWRAP);
    let out = call(&engine, "wrap", vec![BexExternalValue::Int(7)])
        .await
        .unwrap();
    match out {
        BexExternalValue::Instance { class_name, fields } => {
            assert!(class_name.contains("Wrapper"), "got class {class_name}");
            assert_eq!(fields.get("value"), Some(&BexExternalValue::Int(7)));
        }
        other => panic!("expected Instance, got {other:?}"),
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Explicit type args from the host ($types): bind T at the boundary.
// Observable via reflect.type_of<T>() inside the body, and via
// behavior-dependent generics (G3) like json parsing later.
// ─────────────────────────────────────────────────────────────────────────

const REFLECT_T: &str = r#"
    function type_name_of<T>() -> string {
        reflect.type_of<T>().to_string()
    }
"#;

#[tokio::test]
async fn explicit_type_args_reach_frame() {
    let engine = make_engine(REFLECT_T);
    let out = call_with_type_args(
        &engine,
        "type_name_of",
        vec![],
        vec![baml_type::Ty::String {
            attr: baml_type::TyAttr::default(),
        }],
    )
    .await
    .unwrap();
    assert_eq!(out, BexExternalValue::String("string".into()));
}

/// With no explicit type args, an unbound `T` must still be callable: the
/// boundary seeds `unknown` rather than panicking on a missing frame slot.
#[tokio::test]
async fn missing_type_args_default_to_unknown() {
    let engine = make_engine(REFLECT_T);
    let out = call(&engine, "type_name_of", vec![]).await.unwrap();
    assert_eq!(out, BexExternalValue::String("unknown".into()));
}

/// Control pin: the boundary is permissive for concrete params (it coerces,
/// it does not reject). `$types`-bound TypeVars must match this behavior,
/// not exceed it.
#[tokio::test]
async fn concrete_param_mismatch_is_permissive() {
    let engine = make_engine("function wants_string(x: string) -> string { x }");
    let out = call(&engine, "wants_string", vec![BexExternalValue::Int(5)])
        .await
        .unwrap();
    assert_eq!(out, BexExternalValue::Int(5));
}

/// Binding `T` explicitly must make the `T` positions behave exactly as if
/// the type were declared concretely. The boundary is *permissive* (see the
/// control above), so the observable effect of `$types` on params is
/// coercion behavior. Pin the int->bigint widening: a concrete `bigint`
/// param widens an inbound `Int`, therefore `identity(5, $types={T: bigint})`
/// must produce a Bigint.
#[tokio::test]
async fn explicit_type_args_bind_param_coercion_bigint() {
    let engine = make_engine(IDENTITY);
    let out = call_with_type_args(
        &engine,
        "identity",
        vec![BexExternalValue::Int(5)],
        vec![baml_type::Ty::Bigint {
            attr: baml_type::TyAttr::default(),
        }],
    )
    .await
    .unwrap();
    assert_eq!(
        out,
        BexExternalValue::Bigint(num_bigint::BigInt::from(5)),
        "T bound to bigint should widen the inbound int like a concrete bigint param"
    );
}

const IDENTITY_WITH_CLASS: &str = r#"
    class Profile { name string }
    function identity<T>(x: T) -> T { x }
"#;

/// Binding `T` to a class makes an inbound untagged map promote to an
/// Instance of that class -- same as passing a map to a concretely-declared
/// class parameter.
#[tokio::test]
async fn explicit_type_args_promote_map_to_class() {
    let engine = make_engine(IDENTITY_WITH_CLASS);
    let profile_ty = class_ty(&engine, "Profile");
    let out = call_with_type_args(
        &engine,
        "identity",
        vec![BexExternalValue::Map {
            key_type: baml_type::Ty::String {
                attr: baml_type::TyAttr::default(),
            },
            value_type: baml_type::Ty::BuiltinUnknown {
                attr: baml_type::TyAttr::default(),
            },
            entries: [("name".to_string(), BexExternalValue::String("ada".into()))]
                .into_iter()
                .collect(),
        }],
        vec![profile_ty],
    )
    .await
    .unwrap();
    match out {
        BexExternalValue::Instance { class_name, fields } => {
            assert!(class_name.contains("Profile"), "got class {class_name}");
            assert_eq!(
                fields.get("name"),
                Some(&BexExternalValue::String("ada".into()))
            );
        }
        other => panic!("expected Instance after T=Profile promotion, got {other:?}"),
    }
}
