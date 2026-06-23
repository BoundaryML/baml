//! Inbound generics *inference* (01a/01b): a bare generic call (empty
//! `type_args`) has its `TypeVar`s solved from the argument *values* by the
//! engine, then handed to the unchanged explicit downstream. Mirrors the
//! `call_named` harness of `generics_runtime.rs` but passes EMPTY bindings and
//! asserts inference filled them. Case labels map to `00b3-labeled-cases.md`.

mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue, EngineError, FunctionCallContextBuilder};
use common::compile_for_engine;
use sys_native::SysOpsExt;

/// Call `function` with the given argument values and NO explicit type bindings
/// — the engine must infer every `TypeVar` from the values.
async fn call_infer(
    source: &str,
    function: &str,
    args: Vec<BexExternalValue>,
) -> Result<BexExternalValue, EngineError> {
    call_with_bindings(source, function, args, vec![]).await
}

/// Like `call_infer` but seeds some explicit bindings (for the partial-binding
/// and explicit-wins cases).
async fn call_with_bindings(
    source: &str,
    function: &str,
    args: Vec<BexExternalValue>,
    type_args: Vec<(String, baml_type::RuntimeTy)>,
) -> Result<BexExternalValue, EngineError> {
    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );
    engine
        .call_function(
            function,
            args,
            FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_type_args(type_args.into_iter().collect())
                .build(),
            true,
        )
        .await
}

fn s(v: &str) -> BexExternalValue {
    BexExternalValue::String(v.into())
}

/// A free generic function reflecting its single `TypeVar` as a string — the
/// observable proof of what `T` was bound to.
const IDENTITY: &str = r#"
    function identity<T>(x: T) -> string { reflect.type_of<T>().to_string() }
    function main() -> int { 0 }
"#;

// ── §A: known-type args ────────────────────────────────────────────────────

#[tokio::test]
async fn infer_identity_int() {
    // T1: identity(5) ⇒ T=int.
    let out = call_infer(IDENTITY, "identity", vec![BexExternalValue::Int(5)])
        .await
        .unwrap();
    assert_eq!(out, BexExternalValue::String("int".into()));
}

#[tokio::test]
async fn infer_identity_string_widens() {
    // T2/T45: identity("hi") ⇒ T=string (widened, never a literal).
    let out = call_infer(IDENTITY, "identity", vec![s("hi")])
        .await
        .unwrap();
    assert_eq!(out, BexExternalValue::String("string".into()));
}

#[tokio::test]
async fn infer_identity_bool() {
    let out = call_infer(IDENTITY, "identity", vec![BexExternalValue::Bool(true)])
        .await
        .unwrap();
    assert_eq!(out, BexExternalValue::String("bool".into()));
}

#[tokio::test]
async fn infer_identity_null_binds_null() {
    // T47/T33: a bare null binds T=null (TIR-faithful).
    let out = call_infer(IDENTITY, "identity", vec![BexExternalValue::Null])
        .await
        .unwrap();
    assert_eq!(out, BexExternalValue::String("null".into()));
}

// ── §A: generic instance arg carries wire type_args ────────────────────────

#[tokio::test]
async fn infer_identity_generic_instance() {
    // T4-ish: a fully-bound GenericBox[int] instance ⇒ T = GenericBox<int>.
    let source = r#"
        class GenericBox<T> { value T }
        function identity<T>(x: T) -> string { reflect.type_of<T>().to_string() }
        function main() -> int { 0 }
    "#;
    let arg = BexExternalValue::instance_generic(
        "GenericBox",
        vec![baml_type::RuntimeTy::int()],
        indexmap::IndexMap::from_iter([("value", BexExternalValue::Int(5))]),
    );
    let out = call_infer(source, "identity", vec![arg]).await.unwrap();
    // Exact render of the class type is determined by RuntimeTy Display; assert
    // it mentions the class and its int arg.
    let BexExternalValue::String(rendered) = &out else {
        panic!("expected string, got {out:?}");
    };
    assert!(
        rendered.as_str().contains("GenericBox") && rendered.as_str().contains("int"),
        "unexpected render: {rendered:?}"
    );
}

// ── §B: structural / container solving ─────────────────────────────────────

#[tokio::test]
async fn infer_make_triple_structural() {
    // T6: make_triple(1, ["a","b"], {"k": true}) ⇒ A=int, B=string, C=bool.
    // Body reflects each var so we can assert all three were solved.
    // Reflecting the union `A | B | C` proves all three were solved (a single
    // unbound var would fail Gate A before the body runs). Render mentions each.
    let source = r#"
        function make_triple<A, B, C>(a: A, b: B[], c: map<string, C>) -> string {
            reflect.type_of<A | B | C>().to_string()
        }
        function main() -> int { 0 }
    "#;
    let b = BexExternalValue::Array {
        element_type: baml_type::RuntimeTy::string(),
        items: vec![s("a"), s("b")],
    };
    let c = BexExternalValue::Map {
        key_type: baml_type::RuntimeTy::string(),
        value_type: baml_type::RuntimeTy::bool(),
        entries: indexmap::IndexMap::from_iter([("k".to_string(), BexExternalValue::Bool(true))]),
    };
    let out = call_infer(source, "make_triple", vec![BexExternalValue::Int(1), b, c])
        .await
        .unwrap();
    let BexExternalValue::String(rendered) = &out else {
        panic!("expected string, got {out:?}");
    };
    let r = rendered.as_str();
    assert!(
        r.contains("int") && r.contains("string") && r.contains("bool"),
        "expected A=int, B=string, C=bool in render, got {r:?}"
    );
}

#[tokio::test]
async fn infer_second_of_nested_generic() {
    // T9: second_of<T>(p: GenericPair<int, T>) with GenericPair<int, string> ⇒ T=string.
    let source = r#"
        class GenericPair<A, B> { first A second B }
        function second_of<T>(p: GenericPair<int, T>) -> string { reflect.type_of<T>().to_string() }
        function main() -> int { 0 }
    "#;
    let arg = BexExternalValue::instance_generic(
        "GenericPair",
        vec![baml_type::RuntimeTy::int(), baml_type::RuntimeTy::string()],
        indexmap::IndexMap::from_iter([("first", BexExternalValue::Int(1)), ("second", s("hi"))]),
    );
    let out = call_infer(source, "second_of", vec![arg]).await.unwrap();
    assert_eq!(out, BexExternalValue::String("string".into()));
}

// ── §C: union merge (same var, multiple positions) ─────────────────────────

const CHOOSE: &str = r#"
    function choose<T>(left: T, right: T) -> string { reflect.type_of<T>().to_string() }
    function main() -> int { 0 }
"#;

#[tokio::test]
async fn infer_choose_same_type_merges_to_one() {
    // T14: choose(5, 6) ⇒ T = int (union(int,int) dedups).
    let out = call_infer(
        CHOOSE,
        "choose",
        vec![BexExternalValue::Int(5), BexExternalValue::Int(6)],
    )
    .await
    .unwrap();
    assert_eq!(out, BexExternalValue::String("int".into()));
}

#[tokio::test]
async fn infer_choose_divergent_unions() {
    // T15: choose(5, "a") ⇒ T = int | string. Assert the render mentions both.
    let out = call_infer(CHOOSE, "choose", vec![BexExternalValue::Int(5), s("a")])
        .await
        .unwrap();
    let BexExternalValue::String(rendered) = &out else {
        panic!("expected string, got {out:?}");
    };
    assert!(
        rendered.as_str().contains("int") && rendered.as_str().contains("string"),
        "expected an int|string union render, got {rendered:?}"
    );
}

// ── §D: partial binding (explicit seed + inferred) ─────────────────────────

#[tokio::test]
async fn partial_explicit_seed_then_infer() {
    // T17: make_triple with A explicit, B/C inferred. Explicit A wins.
    let source = r#"
        function make_triple<A, B, C>(a: A, b: B[], c: map<string, C>) -> string {
            reflect.type_of<A | B | C>().to_string()
        }
        function main() -> int { 0 }
    "#;
    let b = BexExternalValue::Array {
        element_type: baml_type::RuntimeTy::string(),
        items: vec![s("x")],
    };
    let c = BexExternalValue::Map {
        key_type: baml_type::RuntimeTy::string(),
        value_type: baml_type::RuntimeTy::bool(),
        entries: indexmap::IndexMap::from_iter([("k".to_string(), BexExternalValue::Bool(true))]),
    };
    // Seed A=string explicitly even though the value is int(1) — explicit wins:
    // the render must carry `bool` (C inferred) but NOT `int` (A is string, not
    // the value's int; B is also string).
    let out = call_with_bindings(
        source,
        "make_triple",
        vec![BexExternalValue::Int(1), b, c],
        vec![("A".to_string(), baml_type::RuntimeTy::string())],
    )
    .await
    .unwrap();
    let BexExternalValue::String(rendered) = &out else {
        panic!("expected string, got {out:?}");
    };
    let r = rendered.as_str();
    assert!(
        r.contains("string") && r.contains("bool") && !r.contains("int"),
        "explicit A=string should win over value int; got {r:?}"
    );
}

#[tokio::test]
async fn explicit_binding_wins_over_inference() {
    // Regression: identity(5) with explicit T=string reports string, not int.
    let out = call_with_bindings(
        IDENTITY,
        "identity",
        vec![BexExternalValue::Int(5)],
        vec![("T".to_string(), baml_type::RuntimeTy::string())],
    )
    .await
    .unwrap();
    assert_eq!(out, BexExternalValue::String("string".into()));
}

// ── §E: must-specify still rejects (return/body-only vars) ─────────────────

#[tokio::test]
async fn body_only_var_still_requires_binding() {
    // T19: one_type_arg() with no value carrying T ⇒ Gate A rejects.
    let source = r#"
        function one_type_arg<T>() -> string { reflect.type_of<T>().to_string() }
        function main() -> int { 0 }
    "#;
    let err = call_infer(source, "one_type_arg", vec![])
        .await
        .expect_err("body-only T cannot be inferred");
    assert!(
        matches!(err, EngineError::TypeMismatch { .. }),
        "got {err:?}"
    );
}

#[tokio::test]
async fn return_only_var_still_requires_binding() {
    // T22: parse_as<T>(source: string) -> T ⇒ T only in return ⇒ Gate A rejects.
    let source = r#"
        function parse_as<T>(source: string) -> T throws string { throw source }
        function main() -> int { 0 }
    "#;
    let err = call_infer(source, "parse_as", vec![s("42")])
        .await
        .expect_err("return-only T cannot be inferred");
    assert!(
        matches!(err, EngineError::TypeMismatch { .. }),
        "got {err:?}"
    );
}

// ── §F: host-only values → rust_type ───────────────────────────────────────

/// An opaque host value with no BAML type. `RustData` stands in for a
/// `HostValue` here (both synthesize as `HostOnly`), exercising the
/// `T = rust_type` binding without a live host bridge.
fn host_only() -> BexExternalValue {
    BexExternalValue::RustData(Arc::new(()))
}

#[tokio::test]
async fn infer_identity_host_only_binds_rust_type() {
    // T24: identity(host_obj) ⇒ T = rust_type; reflect renders the rust type.
    let out = call_infer(IDENTITY, "identity", vec![host_only()])
        .await
        .unwrap();
    let BexExternalValue::String(rendered) = &out else {
        panic!("expected string, got {out:?}");
    };
    // RustType renders as `$rust_type` (qualified `baml.rust.RustType`).
    assert!(
        rendered.as_str().contains("rust_type") || rendered.as_str().contains("rust"),
        "expected a rust-type render, got {rendered:?}"
    );
}

#[tokio::test]
async fn infer_choose_known_and_host_only() {
    // T25: choose(5, host_obj) ⇒ T = int | rust_type.
    let out = call_infer(
        CHOOSE,
        "choose",
        vec![BexExternalValue::Int(5), host_only()],
    )
    .await
    .unwrap();
    let BexExternalValue::String(rendered) = &out else {
        panic!("expected string, got {out:?}");
    };
    assert!(
        rendered.as_str().contains("int") && rendered.as_str().contains("rust"),
        "expected int|rust_type render, got {rendered:?}"
    );
}

// ── §I: nullable-only TypeVar T? ───────────────────────────────────────────

const MAYBE_ID: &str = r#"
    function maybe_id<T>(x: T?) -> string { reflect.type_of<T>().to_string() }
    function main() -> int { 0 }
"#;

#[tokio::test]
async fn infer_maybe_id_present_value() {
    // T32: maybe_id(5) null-strips T? to bare T ⇒ T=int.
    let out = call_infer(MAYBE_ID, "maybe_id", vec![BexExternalValue::Int(5)])
        .await
        .unwrap();
    assert_eq!(out, BexExternalValue::String("int".into()));
}

#[tokio::test]
async fn infer_maybe_id_null_value() {
    // T33: maybe_id(None) ⇒ T=null (TIR-faithful).
    let out = call_infer(MAYBE_ID, "maybe_id", vec![BexExternalValue::Null])
        .await
        .unwrap();
    assert_eq!(out, BexExternalValue::String("null".into()));
}

// ── §H: union with a concrete sibling — NOW IN SCOPE (02a, G5 reversal) ──────
//
// `tag_or_value<T>(x: T | string | null)` — a `TypeVar` buried in a union beside
// concrete members. Previously declared out of scope (G5); `02a` reverses that.
// Inference routes the actual to `T` after subtracting the concrete siblings it
// already satisfies; a string/null actual that a sibling absorbs leaves `T`
// unbound (Gate A then governs).

/// Reflect-bodied variant — isolates the *inference* half (no `match`, so the
/// introspection fix is not exercised here).
const TAG_OR_VALUE_REFLECT: &str = r#"
    function tag_or_value<T>(x: T | string | null) -> string {
        reflect.type_of<T>().to_string()
    }
    function main() -> int { 0 }
"#;

#[tokio::test]
async fn infer_tag_or_value_binds_t_from_int() {
    // T30: tag_or_value(5) ⇒ T=int. The int is not absorbed by the `string`/
    // `null` siblings, so it routes to the lone `TypeVar` member.
    let out = call_infer(
        TAG_OR_VALUE_REFLECT,
        "tag_or_value",
        vec![BexExternalValue::Int(5)],
    )
    .await
    .unwrap();
    assert_eq!(out, BexExternalValue::String("int".into()));
}

#[tokio::test]
async fn infer_tag_or_value_string_arg_leaves_t_unbound() {
    // tag_or_value("hi") ⇒ the `string` sibling absorbs the actual; T stays
    // unbound ⇒ Gate A rejects (sound: the string arm, not T, handles strings).
    let err = call_infer(TAG_OR_VALUE_REFLECT, "tag_or_value", vec![s("hi")])
        .await
        .expect_err("string is absorbed by the concrete sibling; T must stay unbound");
    assert!(
        matches!(err, EngineError::TypeMismatch { .. }),
        "got {err:?}"
    );
}

#[tokio::test]
async fn infer_tag_or_value_null_binds_null() {
    // tag_or_value(None) ⇒ null is not absorbed by the `string` sibling, so it
    // routes to T ⇒ T=null (consistent with the bare-null binding decision).
    let out = call_infer(
        TAG_OR_VALUE_REFLECT,
        "tag_or_value",
        vec![BexExternalValue::Null],
    )
    .await
    .unwrap();
    assert_eq!(out, BexExternalValue::String("null".into()));
}

/// Match-bodied variant — the function in `02a`'s "Current status" that fails to
/// compile today. Exercises the *introspection* fix (the `let v: T` arm must not
/// be reported unreachable) end-to-end: compile + infer + execute.
const TAG_OR_VALUE_MATCH: &str = r#"
    function tag_or_value<T>(x: T | string | null) -> T? {
        match (x) {
            let s: string => null,
            null => null,
            let v: T => v,
        }
    }
    function main() -> int { 0 }
"#;

#[tokio::test]
async fn infer_tag_or_value_match_roundtrips_int() {
    // End-to-end: the match-bodied function compiles (introspection fix), infers
    // T=int (inference fix), and round-trips the value ⇒ tag_or_value(5) == 5.
    let out = call_infer(
        TAG_OR_VALUE_MATCH,
        "tag_or_value",
        vec![BexExternalValue::Int(5)],
    )
    .await
    .unwrap();
    assert_eq!(out, BexExternalValue::Int(5));
}

// ── §F: leaf ADT synth (media / collector) ─────────────────────────────────

#[tokio::test]
async fn infer_identity_media_binds_concrete_media_type() {
    // Regression: a media argument is a concrete leaf BAML type, so
    // identity<T>(image) must bind T = image (not the host-only rust_type that
    // the Adt(_) catch-all previously synthesized).
    use bex_engine::BexExternalValue as V;
    use bex_external_types::{BexExternalAdt, MediaKind};
    let media = baml_builtins2::MediaValue::from_url(
        MediaKind::Image,
        "http://example.com/y.png",
        Some("image/png"),
    );
    let arg = V::Adt(BexExternalAdt::Media(media));
    let out = call_infer(IDENTITY, "identity", vec![arg]).await.unwrap();
    assert_eq!(out, BexExternalValue::String("image".into()));
}

#[tokio::test]
async fn infer_nonempty_map_binds_key_despite_valueless_values() {
    // Regression: a NON-empty map whose values give no evidence (an empty array)
    // still carries key evidence — every wire entry is string-keyed — so the
    // map-key TypeVar K must bind to `string` rather than collapsing the whole
    // map to NoEvidence (which previously left K unbound and Gate-A rejected).
    use bex_engine::BexExternalValue as V;
    let src = r#"
        function map_key<K>(m: map<K, int[]>) -> string { reflect.type_of<K>().to_string() }
        function main() -> int { 0 }
    "#;
    let mut entries = indexmap::IndexMap::new();
    entries.insert(
        "a".to_string(),
        V::Array {
            element_type: baml_type::RuntimeTy::int(),
            items: vec![],
        },
    );
    let arg = V::Map {
        key_type: baml_type::RuntimeTy::string(),
        value_type: baml_type::RuntimeTy::List(
            Box::new(baml_type::RuntimeTy::int()),
            baml_type::TyAttr::default(),
        ),
        entries,
    };
    let out = call_infer(src, "map_key", vec![arg]).await.unwrap();
    assert_eq!(out, BexExternalValue::String("string".into()));
}

// ── §H: known-but-deferred Gate A holes (class-method body-only TypeVars) ───

#[tokio::test]
#[ignore = "BUG (deferred): Gate A check(1) is skipped for ALL class methods \
            (lib.rs:2045), so a method's OWN body-only TypeVar (`T` only in \
            reflect.type_of<T>, never in a param/return) is never demanded and \
            silently erases to `unknown`. The free-function analogue IS rejected. \
            A minimal fix needs the enclosing-class generic-param count to split \
            display_type_params into class-prefix vs method-own, which is not \
            carried on the runtime Class/Function objects — plumbing beyond scope."]
async fn gatea_class_method_body_only_var_should_reject() {
    let src = r#"
        class Helper {
            function reflect_t<T>() -> string { reflect.type_of<T>().to_string() }
        }
        function main() -> int { 0 }
    "#;
    let out = call_infer(src, "Helper.reflect_t", vec![]).await;
    assert!(
        matches!(out, Err(EngineError::TypeMismatch { .. })),
        "expected Gate A rejection of body-only T, got {out:?}"
    );
}

#[tokio::test]
#[ignore = "BUG (deferred / not SDK-reachable): an Instance with EMPTY wire \
            type_args for a *generic* class synthesizes Class(GenericBox, []) and \
            binds T = `GenericBox<>` instead of the host-only rust_type that M18 / \
            00b3 T27-T29 require for an unbound generic class. The shipping Python \
            SDK encodes unbound generic classes as HostValue (→ rust_type), not as \
            an empty-args Instance, so this path is only reachable by a fabricated \
            BEX value; the fix needs a decision on the semantics of a zero-arg \
            generic Instance (reject vs treat-as-host-only)."]
async fn unbound_generic_instance_should_be_host_only() {
    let source = r#"
        class GenericBox<T> { value T }
        function identity<T>(x: T) -> string { reflect.type_of<T>().to_string() }
        function main() -> int { 0 }
    "#;
    // Instance whose wire type_args are EMPTY (unbound generic class encoding).
    let arg = BexExternalValue::instance(
        "GenericBox",
        indexmap::IndexMap::from_iter([("value", BexExternalValue::Int(5))]),
    );
    let out = call_infer(source, "identity", vec![arg]).await.unwrap();
    // Expected per M18: T = rust_type (host-only), not `GenericBox<>`.
    assert_eq!(out, BexExternalValue::String("$rust_type".into()));
}

// ── §I: known-but-deferred spec discrepancy (M27 null-sibling absorption) ───

#[tokio::test]
#[ignore = "SPEC DISCREPANCY (deferred / contested decision): per 02a matrix M27, \
            tag_or_value(None) on `T | string | null` should bind NOTHING for T \
            (null is absorbed by the explicit `null` sibling) ⇒ Gate A rejects the \
            reflect-body form. The shipped implementation deliberately binds \
            T = null (test infer_tag_or_value_null_binds_null, comment cites the \
            bare-null decision #1). M27 vs M20 is flagged as `the crisp pin` in the \
            matrix but M27 is still marked FAIL/not-yet-implemented; flipping it \
            reverses a documented shipped choice, so it needs a human decision."]
async fn tag_or_value_null_should_bind_nothing_m27() {
    let out = call_infer(
        TAG_OR_VALUE_REFLECT,
        "tag_or_value",
        vec![BexExternalValue::Null],
    )
    .await;
    assert!(
        matches!(out, Err(EngineError::TypeMismatch { .. })),
        "M27: null absorbed by null sibling ⇒ T unbound ⇒ reject; got {out:?}"
    );
}
