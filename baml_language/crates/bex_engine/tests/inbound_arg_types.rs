//! Host-call argument validation: a host value that is not a member of its
//! declared parameter type is rejected at the call boundary as a
//! `TypeMismatch`, before the VM runs type-specialized code (`AddInt`, typed
//! field loads) that trusts the declared type. Covers the argument itself and
//! every node inside it — list elements, map values, class fields, alias
//! bodies (BoundaryML/baml#5332). The engine converts nothing: a conversion a
//! language allows (an `int` where a `float` is declared) is the host's to
//! make before it states the value.

mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue, EngineError, FunctionCallContextBuilder};
use bex_external_types::{BexExternalAdt, WireTy};
use bex_vm_types::{MediaContent, MediaValue};
use common::compile_for_engine;
use indexmap::IndexMap;
use sys_native::SysOpsExt;

const SOURCE: &str = r#"
    class Counter { n int }
    type Ids = int[]

    function plus(value: int) -> int { value + 1 }
    function half(value: float) -> float { value / 2.0 }
    function maybe_plus(value: int?) -> int {
        if (value == null) { 0 } else { value + 1 }
    }
    function first_plus(values: int[]) -> int { values[0] + 1 }
    function first_id_plus(ids: Ids) -> int { ids[0] + 1 }
    function counter_plus(counter: Counter) -> int { counter.n + 1 }
    function counters_plus(counters: Counter[]) -> int { counters[0].n + 1 }
    function entry_count(entries: map<string, int>) -> int { entries.length() }
    enum Mood { Happy Sad }
    enum Weather { Happy }
    function happy_only(mood: Mood.Happy) -> int { 1 }
    function make_ids() -> int[] { [1, 2] }
    class Label { n string }
    function make_counter() -> Counter { Counter { n: 1 } }
    function make_label() -> Label { Label { n: "one" } }
    function pdf_mime(doc: pdf) -> string? { doc.mime_type() }
    function main() -> int { 0 }
"#;

fn engine() -> Arc<BexEngine> {
    let snapshot = compile_for_engine(SOURCE);
    Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    )
}

async fn call_on(
    engine: &Arc<BexEngine>,
    function: &str,
    args: Vec<BexExternalValue>,
    copy_objects: bool,
) -> Result<BexExternalValue, EngineError> {
    engine
        .call_function(
            function,
            args,
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            copy_objects,
        )
        .await
}

async fn call(
    function: &str,
    args: Vec<BexExternalValue>,
) -> Result<BexExternalValue, EngineError> {
    call_on(&engine(), function, args, true).await
}

/// Assert the call is rejected as a `TypeMismatch` whose message contains each
/// of `needles`.
async fn assert_rejected(function: &str, args: Vec<BexExternalValue>, needles: &[&str]) {
    assert_mismatch(call(function, args).await, needles);
}

fn assert_mismatch(result: Result<BexExternalValue, EngineError>, needles: &[&str]) {
    let err = result.expect_err("a mistyped host argument must be rejected");
    let EngineError::TypeMismatch { message } = &err else {
        panic!("expected TypeMismatch, got {err:?}");
    };
    for needle in needles {
        assert!(
            message.contains(needle),
            "missing {needle:?} in TypeMismatch message: {message:?}"
        );
    }
}

fn string(value: &str) -> BexExternalValue {
    BexExternalValue::String(value.into())
}

fn list(element_type: WireTy, items: Vec<BexExternalValue>) -> BexExternalValue {
    BexExternalValue::Array {
        element_type,
        items,
    }
}

fn counter(n: BexExternalValue) -> BexExternalValue {
    BexExternalValue::instance("Counter", IndexMap::from_iter([("n", n)]))
}

#[tokio::test]
async fn int_param_accepts_int() {
    let out = call("plus", vec![BexExternalValue::Int(41)]).await.unwrap();
    assert_eq!(out, BexExternalValue::Int(42));
}

#[tokio::test]
async fn int_param_rejects_string() {
    assert_rejected(
        "plus",
        vec![string("bad")],
        &["`plus`", "argument `value`", "'string'", "`int`"],
    )
    .await;
}

#[tokio::test]
async fn int_param_rejects_bool_and_float() {
    assert_rejected("plus", vec![BexExternalValue::Bool(true)], &["'bool'"]).await;
    assert_rejected("plus", vec![BexExternalValue::Float(1.5)], &["'float'"]).await;
}

#[tokio::test]
async fn float_param_takes_a_float_and_refuses_an_int() {
    let out = call("half", vec![BexExternalValue::Float(3.0)])
        .await
        .unwrap();
    assert_eq!(out, BexExternalValue::Float(1.5));
    // Widening is the host's conversion, made before it states the value.
    assert_rejected(
        "half",
        vec![BexExternalValue::Int(3)],
        &["`half`", "'int'", "`float`"],
    )
    .await;
}

#[tokio::test]
async fn optional_int_param() {
    let out = call("maybe_plus", vec![BexExternalValue::Null])
        .await
        .unwrap();
    assert_eq!(out, BexExternalValue::Int(0));
    let out = call("maybe_plus", vec![BexExternalValue::Int(1)])
        .await
        .unwrap();
    assert_eq!(out, BexExternalValue::Int(2));
    // Union selection already rejects a value no member admits.
    assert_rejected("maybe_plus", vec![string("bad")], &["string"]).await;
}

#[tokio::test]
async fn list_param_checks_elements() {
    let out = call(
        "first_plus",
        vec![list(WireTy::int(), vec![BexExternalValue::Int(1)])],
    )
    .await
    .unwrap();
    assert_eq!(out, BexExternalValue::Int(2));
    assert_rejected(
        "first_plus",
        vec![list(WireTy::string(), vec![string("bad")])],
        &["'string[]'", "`int[]`"],
    )
    .await;
    // A node that is not what its container states is refused where it sits.
    assert_rejected(
        "first_plus",
        vec![list(WireTy::int(), vec![string("bad")])],
        &["`first_plus`", "'string'", "`int`"],
    )
    .await;
    assert_rejected("first_plus", vec![string("bad")], &["`first_plus`"]).await;
}

#[tokio::test]
async fn alias_param_checks_elements() {
    let out = call(
        "first_id_plus",
        vec![list(WireTy::int(), vec![BexExternalValue::Int(1)])],
    )
    .await
    .unwrap();
    assert_eq!(out, BexExternalValue::Int(2));
    assert_rejected(
        "first_id_plus",
        vec![list(WireTy::string(), vec![string("bad")])],
        &["'string[]'", "`int[]`"],
    )
    .await;
}

#[tokio::test]
async fn map_param_checks_values() {
    let entries = |value| BexExternalValue::Map {
        key_type: WireTy::string(),
        value_type: WireTy::int(),
        entries: IndexMap::from_iter([("a".to_string(), value)]),
    };
    let out = call("entry_count", vec![entries(BexExternalValue::Int(1))])
        .await
        .unwrap();
    assert_eq!(out, BexExternalValue::Int(1));
    assert_rejected("entry_count", vec![entries(string("bad"))], &["'string'"]).await;
}

#[tokio::test]
async fn class_param_checks_fields() {
    let out = call("counter_plus", vec![counter(BexExternalValue::Int(1))])
        .await
        .unwrap();
    assert_eq!(out, BexExternalValue::Int(2));
    assert_rejected(
        "counter_plus",
        vec![counter(string("bad"))],
        &["'string'", "`int`"],
    )
    .await;
    assert_rejected(
        "counters_plus",
        vec![list(WireTy::Unknown, vec![counter(string("bad"))])],
        &["'string'", "`int`"],
    )
    .await;
    assert_rejected(
        "counter_plus",
        vec![BexExternalValue::Int(1)],
        &["`counter_plus`"],
    )
    .await;
}

#[tokio::test]
async fn heap_handle_is_checked_by_what_it_resolves_to() {
    let engine = engine();
    let ids = call_on(&engine, "make_ids", vec![], false).await.unwrap();
    assert!(
        matches!(ids, BexExternalValue::Handle(_)),
        "expected a heap handle, got {ids:?}"
    );
    let out = call_on(&engine, "first_plus", vec![ids.clone()], true)
        .await
        .unwrap();
    assert_eq!(out, BexExternalValue::Int(2));
    assert_mismatch(
        call_on(&engine, "plus", vec![ids], true).await,
        &["`plus`", "'int[]'", "`int`"],
    );
}

#[tokio::test]
async fn enum_variant_param_requires_the_declared_member() {
    let out = call(
        "happy_only",
        vec![BexExternalValue::variant("Mood", "Happy")],
    )
    .await
    .unwrap();
    assert_eq!(out, BexExternalValue::Int(1));
    assert_rejected(
        "happy_only",
        vec![BexExternalValue::variant("Mood", "Sad")],
        &["`happy_only`"],
    )
    .await;
    assert_rejected(
        "happy_only",
        vec![BexExternalValue::variant("Weather", "Happy")],
        &["`happy_only`"],
    )
    .await;
}

#[tokio::test]
async fn handle_to_another_class_is_rejected() {
    let engine = engine();
    let counter = call_on(&engine, "make_counter", vec![], false)
        .await
        .unwrap();
    let label = call_on(&engine, "make_label", vec![], false).await.unwrap();
    let out = call_on(&engine, "counter_plus", vec![counter], true)
        .await
        .unwrap();
    assert_eq!(out, BexExternalValue::Int(2));
    assert_mismatch(
        call_on(&engine, "counter_plus", vec![label], true).await,
        &["`counter_plus`", "'Label'", "`Counter`"],
    );
}

fn pdf() -> BexExternalValue {
    BexExternalValue::Adt(BexExternalAdt::Media(Arc::new(MediaValue::new(
        baml_type::MediaKind::Pdf,
        MediaContent::Url {
            url: "https://example.test/sample.pdf".to_string(),
            base64_data: None,
        },
        Some("application/pdf".to_string()),
    ))))
}

#[tokio::test]
async fn media_and_class_slots_do_not_cross() {
    let out = call("pdf_mime", vec![pdf()]).await.unwrap();
    assert_eq!(out, BexExternalValue::String("application/pdf".into()));
    assert_rejected("counter_plus", vec![pdf()], &["Counter"]).await;
    let map = BexExternalValue::Map {
        key_type: WireTy::string(),
        value_type: WireTy::Unknown,
        entries: IndexMap::from_iter([("_data".to_string(), pdf())]),
    };
    assert_rejected("pdf_mime", vec![map], &["`pdf_mime`"]).await;
}
