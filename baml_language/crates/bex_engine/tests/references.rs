//! What a host holds of the runtime's own. An instance that keeps runtime data
//! in a `$rust_type` field crosses by its fields, with that data as a payload
//! the host passes back as it received it. An instance of a `@@rust_type`
//! class and a standard-library capability (`reflect.Package`) cross as an
//! opaque reference to the engine's instance, which a host holds, passes back
//! as that same instance, and never builds.

mod common;

use std::sync::Arc;

use baml_type::TypeName;
use bex_engine::{BexEngine, BexExternalValue, EngineError, FunctionCallContextBuilder};
use bex_external_types::{BexExternalAdt, TaggedHeapHandleKind, WireHead};
use common::compile_for_engine;
use indexmap::IndexMap;
use sys_native::SysOpsExt;

const SOURCE: &str = r#"
    class Marker {
        n int
    }

    function make_token() -> baml.spawn.CancelToken {
        baml.spawn.CancelToken.new()
    }

    function cancel_token(token: baml.spawn.CancelToken) -> null {
        let _ = token.cancel();
        null
    }

    function is_cancelled(token: baml.spawn.CancelToken) -> bool {
        token.is_cancelled()
    }

    function make_hasher() -> baml.hash.DefaultHasher {
        baml.hash.DefaultHasher.new()
    }

    function hash_after(hasher: baml.hash.DefaultHasher, text: string) -> int {
        hasher.write(text.to_utf8());
        hasher.finish()
    }

    function hash_text(text: string) -> int {
        hash_after(baml.hash.DefaultHasher.new(), text)
    }

    function pick(hasher: bool) -> baml.hash.DefaultHasher | Marker {
        if (hasher) {
            baml.hash.DefaultHasher.new()
        } else {
            Marker { n: 1 }
        }
    }

    function make_identity() -> (x: int) -> int throws never {
        (x: int) -> int { x }
    }

    function current_package() -> reflect.Package {
        reflect.Package.current()
    }

    function same_package(package: reflect.Package) -> reflect.Package {
        package
    }

    function main() -> int { 0 }
"#;

fn engine() -> Arc<BexEngine> {
    let snapshot = compile_for_engine(SOURCE);
    Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    )
}

async fn call(
    engine: &Arc<BexEngine>,
    function: &str,
    args: Vec<BexExternalValue>,
) -> Result<BexExternalValue, EngineError> {
    engine
        .call_function(
            function,
            args,
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
}

fn string(value: &str) -> BexExternalValue {
    BexExternalValue::String(value.into())
}

/// The refusal's message, or a panic with whatever else came back.
#[track_caller]
fn refusal(result: Result<BexExternalValue, EngineError>) -> String {
    match result {
        Err(EngineError::TypeMismatch { message }) => message,
        other => panic!("expected a type mismatch, got {other:?}"),
    }
}

/// `baml.hash.DefaultHasher` as a wire type, in either the engine's
/// annotation form or the host's.
fn default_hasher_ty<L: Clone>() -> baml_type::WireTy<L> {
    baml_type::RealizedTy::Class(
        WireHead::Static(TypeName::from_dotted_path("baml.hash.DefaultHasher")),
        Box::new([]),
    )
}

/// The value is a live reference to an instance of `baml.hash.DefaultHasher`.
#[track_caller]
fn assert_hasher_reference(value: &BexExternalValue) {
    let BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle { kind, ty, .. }) = value else {
        panic!("a hasher crosses as a live reference, got {value:?}");
    };
    assert_eq!(*kind, TaggedHeapHandleKind::RuntimeValue);
    assert_eq!(*ty, default_hasher_ty());
}

/// An instance holding runtime data in a field crosses by its fields, the
/// data among them as a payload the engine issued, and works when it comes
/// back.
#[tokio::test]
async fn an_instance_holding_runtime_data_crosses_by_fields_with_its_payload() {
    let engine = engine();
    let token = call(&engine, "make_token", vec![]).await.unwrap();
    let BexExternalValue::Instance {
        class_name, fields, ..
    } = &token
    else {
        panic!("a CancelToken crosses by its fields, got {token:?}");
    };
    assert_eq!(class_name, "baml.spawn.CancelToken");
    assert!(
        matches!(fields.get("_handle"), Some(BexExternalValue::RustData(_))),
        "its state is a payload: {fields:?}"
    );
    // The payload is the engine's own: cancelling through one crossing is
    // seen through the next.
    call(&engine, "cancel_token", vec![token.clone()])
        .await
        .unwrap();
    let cancelled = call(&engine, "is_cancelled", vec![token]).await.unwrap();
    assert_eq!(cancelled, BexExternalValue::Bool(true));
    // Negative control: a token nobody cancelled is not cancelled.
    let fresh = call(&engine, "make_token", vec![]).await.unwrap();
    let fresh_cancelled = call(&engine, "is_cancelled", vec![fresh]).await.unwrap();
    assert_eq!(fresh_cancelled, BexExternalValue::Bool(false));
}

/// An instance of a `@@rust_type` class keeps all its state in the runtime,
/// so it crosses as a reference to the engine's instance.
#[tokio::test]
async fn a_rust_type_instance_crosses_as_a_reference() {
    let engine = engine();
    let hasher = call(&engine, "make_hasher", vec![]).await.unwrap();
    assert_hasher_reference(&hasher);
}

/// A host passes the reference back as it received it, and the engine gets
/// the same instance: state one call leaves is the next call's.
#[tokio::test]
async fn a_rust_type_instance_comes_back_as_the_same_instance() {
    let engine = engine();
    let hasher = call(&engine, "make_hasher", vec![]).await.unwrap();
    // The second call's hash covers both writes, as one hasher fed "ab" does.
    call(&engine, "hash_after", vec![hasher.clone(), string("a")])
        .await
        .unwrap();
    let after_both = call(&engine, "hash_after", vec![hasher, string("b")])
        .await
        .unwrap();
    let fresh = call(&engine, "hash_text", vec![string("ab")])
        .await
        .unwrap();
    assert_eq!(after_both, fresh);
    // Negative control: a fresh hasher fed only "b" differs, so the equality
    // above came from the shared state.
    let only_b = call(&engine, "hash_text", vec![string("b")]).await.unwrap();
    assert_ne!(after_both, only_b);
}

/// A host never builds an instance of a `@@rust_type` class: it has no
/// fields, and its state is the runtime's own.
#[tokio::test]
async fn a_host_never_builds_a_rust_type_instance() {
    let engine = engine();
    let forged = BexExternalValue::Instance {
        class_name: "baml.hash.DefaultHasher".to_string(),
        type_args: Vec::new(),
        fields: IndexMap::new(),
    };
    let message = refusal(call(&engine, "hash_after", vec![forged, string("a")]).await);
    assert!(
        message.contains("`baml.hash.DefaultHasher` is the runtime's own"),
        "{message}"
    );
}

/// A reference fills a slot only if the instance it names is a member: a
/// closure is not a `DefaultHasher`.
#[tokio::test]
async fn a_reference_of_another_type_does_not_fill_a_rust_type_slot() {
    let engine = engine();
    let closure = call(&engine, "make_identity", vec![]).await.unwrap();
    let message = refusal(call(&engine, "hash_after", vec![closure, string("a")]).await);
    assert!(
        message.contains("does not match the declared type `baml.hash.DefaultHasher`"),
        "{message}"
    );
}

/// A reference under a union return carries the arm its instance selects.
#[tokio::test]
async fn a_rust_type_instance_selects_its_union_arm() {
    let engine = engine();
    let picked = call(&engine, "pick", vec![BexExternalValue::Bool(true)])
        .await
        .unwrap();
    let BexExternalValue::Union { value, metadata } = &picked else {
        panic!("a union return carries its selected arm, got {picked:?}");
    };
    assert_eq!(metadata.selected_option, default_hasher_ty());
    assert_hasher_reference(value);
}

/// A standard-library capability whose fields are the runtime's own crosses
/// as an opaque reference, and comes back as the same instance.
#[tokio::test]
async fn a_capability_crosses_as_an_opaque_reference_and_comes_back() {
    let engine = engine();
    let package = call(&engine, "current_package", vec![]).await.unwrap();
    let BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
        kind: TaggedHeapHandleKind::RuntimeValue,
        ty,
        heap_handle,
    }) = &package
    else {
        panic!("a reflect.Package crosses as an opaque reference, got {package:?}");
    };
    assert_eq!(ty.to_string(), "reflect.Package");
    let returned = call(&engine, "same_package", vec![package.clone()])
        .await
        .unwrap();
    let BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
        heap_handle: returned,
        ..
    }) = &returned
    else {
        panic!("the package comes back as a reference, got {returned:?}");
    };
    assert_eq!(
        returned, heap_handle,
        "the instance that comes back is the one that went in"
    );
}

/// A host never builds a capability from fields: its fields are the
/// runtime's internals, which no host value stands in for.
#[tokio::test]
async fn a_host_never_builds_a_capability() {
    let engine = engine();
    let forged = BexExternalValue::instance(
        "reflect.Package",
        IndexMap::from_iter([("_inner", BexExternalValue::Null)]),
    );
    let error = call(&engine, "same_package", vec![forged])
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("is the runtime's own; pass the instance you received"),
        "a by-fields reflect.Package is refused: {error}"
    );
}

/// A reference names an instance of the runtime that made it; another
/// runtime cannot reach that instance and refuses the reference.
#[tokio::test]
async fn a_reference_from_another_runtime_is_refused() {
    let origin = engine();
    let package = call(&origin, "current_package", vec![]).await.unwrap();
    let other = engine();
    let error = call(&other, "same_package", vec![package])
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("different BAML runtime"),
        "a reference into another runtime is refused: {error}"
    );
}
