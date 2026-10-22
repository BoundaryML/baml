//! What a host holds of the runtime's own: an instance that keeps runtime
//! data (`$rust_type`) crosses by its fields with that data as a payload the
//! host passes back as it received it, and a standard-library capability
//! (`reflect.Package`) crosses as an opaque reference to the engine's
//! instance, which a host holds, passes back, and never builds.

mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue, EngineError, FunctionCallContextBuilder};
use bex_external_types::{BexExternalAdt, TaggedHeapHandleKind};
use common::compile_for_engine;
use indexmap::IndexMap;
use sys_native::SysOpsExt;

const SOURCE: &str = r#"
    function make_hasher() -> baml.hash.DefaultHasher {
        baml.hash.DefaultHasher.new()
    }

    function hasher_write(hasher: baml.hash.DefaultHasher, text: string) -> null {
        hasher.write(text.to_utf8());
        null
    }

    function hasher_finish(hasher: baml.hash.DefaultHasher) -> int {
        hasher.finish()
    }

    function hash_text(text: string) -> int {
        let hasher = baml.hash.DefaultHasher.new();
        hasher.write(text.to_utf8());
        hasher.finish()
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

/// An instance holding runtime data crosses by its fields, the data among
/// them as a payload the engine issued.
#[tokio::test]
async fn an_instance_holding_runtime_data_crosses_by_fields_with_its_payload() {
    let engine = engine();
    let hasher = call(&engine, "make_hasher", vec![]).await.unwrap();
    let BexExternalValue::Instance {
        class_name, fields, ..
    } = &hasher
    else {
        panic!("a DefaultHasher crosses by its fields, got {hasher:?}");
    };
    assert_eq!(class_name, "baml.hash.DefaultHasher");
    assert!(
        matches!(fields.get("_state"), Some(BexExternalValue::RustData(_))),
        "its state is a payload: {fields:?}"
    );
}

/// A host passes the instance back as it received it; the payload is the
/// engine's own, so the instance the engine builds shares its state.
#[tokio::test]
async fn a_payload_sent_back_keeps_its_state() {
    let engine = engine();
    let hasher = call(&engine, "make_hasher", vec![]).await.unwrap();
    call(&engine, "hasher_write", vec![hasher.clone(), string("ab")])
        .await
        .unwrap();
    call(&engine, "hasher_write", vec![hasher.clone(), string("cd")])
        .await
        .unwrap();
    let finished = call(&engine, "hasher_finish", vec![hasher]).await.unwrap();
    let expected = call(&engine, "hash_text", vec![string("abcd")])
        .await
        .unwrap();
    assert_eq!(
        finished, expected,
        "two calls wrote through one payload: the state the first left is the second's"
    );
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
