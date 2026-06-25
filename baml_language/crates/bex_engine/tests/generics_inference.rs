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
async fn infer_identity_null_binds_rust_type() {
    // §I I4 (decided): a bare `null` actual gives the value position no concrete
    // leaf, so we do NOT bind `T = null`; `T` defaults to host-only `rust_type`
    // (rule 4) and the value round-trips unchanged.
    let out = call_infer(IDENTITY, "identity", vec![BexExternalValue::Null])
        .await
        .unwrap();
    assert_eq!(out, BexExternalValue::String("$rust_type".into()));
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
    let EngineError::TypeMismatch { message } = &err else {
        panic!("expected TypeMismatch, got {err:?}");
    };
    // The message must read for a non-type-theorist: name the function, the var,
    // and how to specify it via subscript; no `<:`/variance jargon, and no
    // `_types=` (an internal wiring detail, not a user-facing surface).
    assert!(
        message.contains("one_type_arg")
            && message.contains('T')
            && message.contains("one_type_arg[int]"),
        "unfriendly must-specify message: {message:?}"
    );
    assert!(
        !message.contains("<:")
            && !message.to_lowercase().contains("covariant")
            && !message.contains("_types"),
        "message leaks jargon or internal `_types=` syntax: {message:?}"
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

// ── §J: empty collections → rust_type element/value ────────────────────────

#[tokio::test]
async fn infer_identity_empty_array_binds_rust_type_list() {
    // An empty `[]` carries no element evidence, but it still inhabits a list:
    // `identity([])` ⇒ T = rust_type[], NOT a Gate-A rejection.
    let arg = BexExternalValue::Array {
        element_type: baml_type::RuntimeTy::int(),
        items: vec![],
    };
    let out = call_infer(IDENTITY, "identity", vec![arg]).await.unwrap();
    let BexExternalValue::String(rendered) = &out else {
        panic!("expected string, got {out:?}");
    };
    let r = rendered.as_str();
    assert!(
        r.contains("rust") && r.contains("[]"),
        "expected a rust_type list render, got {r:?}"
    );
}

#[tokio::test]
async fn infer_identity_empty_map_binds_rust_type_map() {
    // A genuinely empty `{}` carries no value evidence, but every wire entry is
    // string-keyed: `identity({})` ⇒ T = map<string, rust_type>, not rejected.
    let arg = BexExternalValue::Map {
        key_type: baml_type::RuntimeTy::string(),
        value_type: baml_type::RuntimeTy::int(),
        entries: indexmap::IndexMap::new(),
    };
    let out = call_infer(IDENTITY, "identity", vec![arg]).await.unwrap();
    let BexExternalValue::String(rendered) = &out else {
        panic!("expected string, got {out:?}");
    };
    let r = rendered.as_str();
    assert!(
        r.contains("map") && r.contains("string") && r.contains("rust"),
        "expected a map<string, rust_type> render, got {r:?}"
    );
}

// (An unresolved-enum `Variant` now synthesizes host-only `rust_type` too — the
// last `NoEvidence` producer to go — but it can't be exercised end-to-end here:
// value conversion rejects an unknown enum name before the call materializes.)

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
    // §I I4 (decided): maybe_id(None) does NOT null-strip `T?` to bind `T=null`;
    // a `null`-only actual is no evidence ⇒ `T` defaults to `rust_type`.
    let out = call_infer(MAYBE_ID, "maybe_id", vec![BexExternalValue::Null])
        .await
        .unwrap();
    assert_eq!(out, BexExternalValue::String("$rust_type".into()));
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
async fn infer_tag_or_value_string_arg_binds_rust_type() {
    // §H H3 (decided): tag_or_value("hi") ⇒ the `string` sibling absorbs the
    // actual, so the `T` arm gets no residual evidence. `T` still has a value
    // position (the `x` param) and no closure occurrence, so it defaults to
    // `rust_type` (rule 4) rather than being rejected.
    let out = call_infer(TAG_OR_VALUE_REFLECT, "tag_or_value", vec![s("hi")])
        .await
        .unwrap();
    assert_eq!(out, BexExternalValue::String("$rust_type".into()));
}

#[tokio::test]
async fn infer_tag_or_value_null_binds_rust_type() {
    // §H H3 (decided): tag_or_value(None) ⇒ a `null` actual is no evidence (it is
    // not bound as `T=null`), and the `null` sibling absorbs it, so `T` gets no
    // residual ⇒ defaults to `rust_type` (rule 4).
    let out = call_infer(
        TAG_OR_VALUE_REFLECT,
        "tag_or_value",
        vec![BexExternalValue::Null],
    )
    .await
    .unwrap();
    assert_eq!(out, BexExternalValue::String("$rust_type".into()));
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

// ── §J: variance soundness at the value seam (02d/02e) ──────────────────────
//
// The variance-aware checked solver (`baml_type_runtime::InferenceConstraints`)
// runs over ALL arguments at once, so a `TypeVar` used at conflicting variances
// is rejected with a `TypeMismatch` rather than fabricated into an unsound
// union. Only the *invariant-container* shapes (§J E1–E4 / E2 / E3) are
// reachable end-to-end here — a function-typed actual crosses the FFI as an
// opaque handle, never a structural `Ty::Function`, so the contravariant
// function-param cases (J1–J3, J8) live only at the unifier layer (and are
// reified at the bridge per J13). See `02e2` for the layer mapping.

fn arr(element_type: baml_type::RuntimeTy, items: Vec<BexExternalValue>) -> BexExternalValue {
    BexExternalValue::Array {
        element_type,
        items,
    }
}

fn int_map(value_type: baml_type::RuntimeTy, entries: &[(&str, BexExternalValue)]) -> BexExternalValue {
    BexExternalValue::Map {
        key_type: baml_type::RuntimeTy::string(),
        value_type,
        entries: entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    }
}

/// `pair<T>(a: T[], b: T[])` — `T` in two **invariant** (list-element) positions.
const PAIR: &str = r#"
    function pair<T>(a: T[], b: T[]) -> string { reflect.type_of<T>().to_string() }
    function main() -> int { 0 }
"#;

#[tokio::test]
async fn infer_pair_invariant_list_conflict_rejects() {
    // J4/E1: pair(int[], string[]) ⇒ a⇒T==int, b⇒T==string (both invariant) ⇒
    // no consistent T ⇒ reject (today's bug fabricated `(int|string)[]`).
    let a = arr(baml_type::RuntimeTy::int(), vec![BexExternalValue::Int(1)]);
    let b = arr(baml_type::RuntimeTy::string(), vec![s("x")]);
    let err = call_infer(PAIR, "pair", vec![a, b])
        .await
        .expect_err("invariant list conflict must reject");
    let EngineError::TypeMismatch { message } = &err else {
        panic!("expected TypeMismatch, got {err:?}");
    };
    // Friendly conflict message: names the function and the clashing concrete
    // types in plain language, with no `<:`/"invariant"/"meet" jargon.
    assert!(
        message.contains("pair")
            && message.contains("int")
            && message.contains("string")
            && message.contains("same type in every argument"),
        "unfriendly conflict message: {message:?}"
    );
    assert!(
        !message.contains("<:")
            && !message.to_lowercase().contains("invariant")
            && !message.to_lowercase().contains("meet"),
        "message leaks type-theory jargon: {message:?}"
    );
}

#[tokio::test]
async fn infer_pair_invariant_list_agree_binds() {
    // J9/G1 regression: pair(int[], int[]) ⇒ two invariant occurrences AGREE ⇒
    // T = int; must still succeed (the fix narrows, it must not over-reject).
    let a = arr(baml_type::RuntimeTy::int(), vec![BexExternalValue::Int(1)]);
    let b = arr(baml_type::RuntimeTy::int(), vec![BexExternalValue::Int(2)]);
    let out = call_infer(PAIR, "pair", vec![a, b]).await.unwrap();
    assert_eq!(out, BexExternalValue::String("int".into()));
}

#[tokio::test]
async fn infer_choose_union_outside_container_joins() {
    // J10/G2 regression: choose(int[], string[]) ⇒ both occurrences are covariant
    // (bare `T`), so the union forms OUTSIDE the container ⇒ T = int[] | string[].
    // Proves the fix keys on position variance, not "arrays are involved."
    let a = arr(baml_type::RuntimeTy::int(), vec![BexExternalValue::Int(1)]);
    let b = arr(baml_type::RuntimeTy::string(), vec![s("x")]);
    let out = call_infer(CHOOSE, "choose", vec![a, b]).await.unwrap();
    let BexExternalValue::String(rendered) = &out else {
        panic!("expected string, got {out:?}");
    };
    let r = rendered.as_str();
    assert!(
        r.contains("int") && r.contains("string") && r.contains("[]"),
        "expected `int[] | string[]` render, got {r:?}"
    );
}

#[tokio::test]
async fn infer_merge_invariant_map_value_conflict_rejects() {
    // J5/E2: merge(map<string,int>, map<string,string>) ⇒ conflicting invariant
    // map-value ⇒ reject.
    let src = r#"
        function merge<T>(a: map<string, T>, b: map<string, T>) -> string {
            reflect.type_of<T>().to_string()
        }
        function main() -> int { 0 }
    "#;
    let a = int_map(baml_type::RuntimeTy::int(), &[("k", BexExternalValue::Int(1))]);
    let b = int_map(baml_type::RuntimeTy::string(), &[("k", s("x"))]);
    let err = call_infer(src, "merge", vec![a, b])
        .await
        .expect_err("invariant map-value conflict must reject");
    assert!(matches!(err, EngineError::TypeMismatch { .. }), "got {err:?}");
}

#[tokio::test]
async fn infer_combine_invariant_class_arg_conflict_rejects() {
    // J6/E3: combine(GenericBox[int], GenericBox[string]) ⇒ Box<T> invariant,
    // int ≠ string ⇒ reject.
    let src = r#"
        class GenericBox<T> { value T }
        function combine<T>(x: GenericBox<T>, y: GenericBox<T>) -> string {
            reflect.type_of<T>().to_string()
        }
        function main() -> int { 0 }
    "#;
    let x = BexExternalValue::instance_generic(
        "GenericBox",
        vec![baml_type::RuntimeTy::int()],
        indexmap::IndexMap::from_iter([("value", BexExternalValue::Int(1))]),
    );
    let y = BexExternalValue::instance_generic(
        "GenericBox",
        vec![baml_type::RuntimeTy::string()],
        indexmap::IndexMap::from_iter([("value", s("x"))]),
    );
    let err = call_infer(src, "combine", vec![x, y])
        .await
        .expect_err("invariant class-arg conflict must reject");
    assert!(matches!(err, EngineError::TypeMismatch { .. }), "got {err:?}");
}

#[tokio::test]
async fn infer_glue_invariant_vs_covariant_conflict_rejects() {
    // J7/E4: glue(int, string[]) ⇒ arr⇒T==string (invariant) but bare⇒int <: T
    // (covariant); int <: string is false ⇒ reject (the key cross-variance case).
    let src = r#"
        function glue<T>(bare: T, arr: T[]) -> string { reflect.type_of<T>().to_string() }
        function main() -> int { 0 }
    "#;
    let arr_arg = arr(baml_type::RuntimeTy::string(), vec![s("x")]);
    let err = call_infer(src, "glue", vec![BexExternalValue::Int(1), arr_arg])
        .await
        .expect_err("invariant×covariant conflict must reject");
    assert!(matches!(err, EngineError::TypeMismatch { .. }), "got {err:?}");
}

#[tokio::test]
async fn infer_glue_invariant_and_covariant_agree_binds() {
    // J11/G4 regression: glue(int, int[]) ⇒ invariant (T==int) + covariant
    // (int <: int) AGREE ⇒ T = int; must succeed.
    let src = r#"
        function glue<T>(bare: T, arr: T[]) -> string { reflect.type_of<T>().to_string() }
        function main() -> int { 0 }
    "#;
    let arr_arg = arr(baml_type::RuntimeTy::int(), vec![BexExternalValue::Int(2)]);
    let out = call_infer(src, "glue", vec![BexExternalValue::Int(1), arr_arg])
        .await
        .unwrap();
    assert_eq!(out, BexExternalValue::String("int".into()));
}

// ── §D: n-ary covariant join ────────────────────────────────────────────────

#[tokio::test]
async fn infer_triple_choose_three_covariant_join() {
    // D3: triple_choose(5, "asdf", True) ⇒ T = int | string | bool — three
    // covariant bare-arg occurrences union-merge (n-ary, not pairwise-special).
    let src = r#"
        function triple_choose<T>(a: T, b: T, c: T) -> string {
            reflect.type_of<T>().to_string()
        }
        function main() -> int { 0 }
    "#;
    let out = call_infer(
        src,
        "triple_choose",
        vec![BexExternalValue::Int(5), s("asdf"), BexExternalValue::Bool(true)],
    )
    .await
    .unwrap();
    let BexExternalValue::String(rendered) = &out else {
        panic!("expected string, got {out:?}");
    };
    let r = rendered.as_str();
    assert!(
        r.contains("int") && r.contains("string") && r.contains("bool"),
        "expected int|string|bool, got {r:?}"
    );
}

// ── §B: heterogeneous container element ⇒ covariant element union-merge ──────

#[tokio::test]
async fn infer_make_triple_heterogeneous_list_element_unions() {
    // B8: make_triple(1, [1, "x"], {"k": True}) ⇒ B = int | string — the list's
    // mixed elements union-merge while synthesizing ONE container's element type
    // (the §D covariant join applied inside a container; distinct from §J's
    // invariant conflict BETWEEN two separate args).
    let src = r#"
        function make_triple<A, B, C>(a: A, b: B[], c: map<string, C>) -> string {
            reflect.type_of<A | B | C>().to_string()
        }
        function main() -> int { 0 }
    "#;
    let b = arr(
        baml_type::RuntimeTy::int(),
        vec![BexExternalValue::Int(1), s("x")],
    );
    let c = int_map(baml_type::RuntimeTy::bool(), &[("k", BexExternalValue::Bool(true))]);
    let out = call_infer(src, "make_triple", vec![BexExternalValue::Int(1), b, c])
        .await
        .unwrap();
    let BexExternalValue::String(rendered) = &out else {
        panic!("expected string, got {out:?}");
    };
    let r = rendered.as_str();
    assert!(
        r.contains("int") && r.contains("string") && r.contains("bool"),
        "expected B=int|string in the A|B|C render, got {r:?}"
    );
}

// ── §J: multiple unconstrained TypeVars in one union ⇒ reject ────────────────

#[tokio::test]
async fn infer_two_typevar_union_is_uninferrable_rejects() {
    // J12: f<T, U>(x: T | U | int) called with any value ⇒ reject as
    // un-inferrable — two free vars in one union have no principled split without
    // an explicit hint (distinct from §H, which is ONE var beside concrete
    // members). Both T and U stay unbound ⇒ Gate A rejects.
    let src = r#"
        function two_in_union<T, U>(x: T | U | int) -> string {
            reflect.type_of<T | U>().to_string()
        }
        function main() -> int { 0 }
    "#;
    let err = call_infer(src, "two_in_union", vec![s("hello")])
        .await
        .expect_err("two free vars in one union are un-inferrable");
    assert!(matches!(err, EngineError::TypeMismatch { .. }), "got {err:?}");
}

// ── §A: known non-generic BAML class actual ─────────────────────────────────

#[tokio::test]
async fn infer_identity_known_class_instance() {
    // A2: identity(StringIntPair(...)) ⇒ T = StringIntPair (a known, non-generic
    // BAML class recovered from the instance value).
    let src = r#"
        class StringIntPair { my_string string my_int int }
        function identity<T>(x: T) -> string { reflect.type_of<T>().to_string() }
        function main() -> int { 0 }
    "#;
    let arg = BexExternalValue::instance(
        "StringIntPair",
        indexmap::IndexMap::from_iter([("my_string", s("a")), ("my_int", BexExternalValue::Int(1))]),
    );
    let out = call_infer(src, "identity", vec![arg]).await.unwrap();
    let BexExternalValue::String(rendered) = &out else {
        panic!("expected string, got {out:?}");
    };
    assert!(
        rendered.as_str().contains("StringIntPair"),
        "expected T = StringIntPair, got {rendered:?}"
    );
}

// ── §B: nested generic-class arg, four vars zipped ──────────────────────────

fn pair_rt(first: baml_type::RuntimeTy, second: baml_type::RuntimeTy) -> baml_type::RuntimeTy {
    baml_type::RuntimeTy::Class(
        baml_type::TypeName::local(baml_type::Name::new("GenericPair")),
        vec![first, second],
        baml_type::TyAttr::default(),
    )
}

#[tokio::test]
async fn infer_extract_four_vars_from_nested_generic() {
    // B5: extract<A,B,C,D>(GenericPair<GenericPair<A,B>, GenericPair<C,D>>) over a
    // fully-bound nested instance ⇒ "int | string | bool | float" — four vars
    // zipped from the nested wire type-args.
    let src = r#"
        class GenericPair<A, B> { first A second B }
        function extract<A, B, C, D>(a: GenericPair<GenericPair<A, B>, GenericPair<C, D>>) -> string {
            reflect.type_of<A | B | C | D>().to_string()
        }
        function main() -> int { 0 }
    "#;
    use baml_type::RuntimeTy;
    let inner1 = BexExternalValue::instance_generic(
        "GenericPair",
        vec![RuntimeTy::int(), RuntimeTy::string()],
        indexmap::IndexMap::from_iter([("first", BexExternalValue::Int(1)), ("second", s("a"))]),
    );
    let inner2 = BexExternalValue::instance_generic(
        "GenericPair",
        vec![RuntimeTy::bool(), RuntimeTy::float()],
        indexmap::IndexMap::from_iter([
            ("first", BexExternalValue::Bool(true)),
            ("second", BexExternalValue::Float(1.5)),
        ]),
    );
    let arg = BexExternalValue::instance_generic(
        "GenericPair",
        vec![
            pair_rt(RuntimeTy::int(), RuntimeTy::string()),
            pair_rt(RuntimeTy::bool(), RuntimeTy::float()),
        ],
        indexmap::IndexMap::from_iter([("first", inner1), ("second", inner2)]),
    );
    let out = call_infer(src, "extract", vec![arg]).await.unwrap();
    let BexExternalValue::String(rendered) = &out else {
        panic!("expected string, got {out:?}");
    };
    let r = rendered.as_str();
    assert!(
        r.contains("int") && r.contains("string") && r.contains("bool") && r.contains("float"),
        "expected all four vars, got {r:?}"
    );
}

// ── §C: explicit binding contradicted by a non-instance actual (PINS reality) ─

#[tokio::test]
async fn explicit_binding_wins_over_contradicting_scalar_actual() {
    // C4 (pins EXISTING behavior; the 03b doc flags this outcome as "confirm what
    // the code really does"): seed A=int explicitly but pass a *string* for `a`.
    // The explicit binding wins and `A` stays `int`; the contradicting scalar `a`
    // value is NOT re-validated against the now-concrete formal at this value seam
    // (Gate B only structurally checks *Instance* args — a bare string against an
    // `int` formal slips through), so the call SUCCEEDS with A=int rather than
    // rejecting. Documenting the gap: the reflect render carries `int` (from the
    // explicit A), `string` (B from ["x"]) and `bool` (C from {k:true}).
    let src = r#"
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
    let out = call_with_bindings(
        src,
        "make_triple",
        vec![s("nope"), b, c],
        vec![("A".to_string(), baml_type::RuntimeTy::int())],
    )
    .await
    .expect("explicit A=int wins; the scalar `a` is not re-validated at this seam");
    let BexExternalValue::String(rendered) = &out else {
        panic!("expected string, got {out:?}");
    };
    let r = rendered.as_str();
    assert!(
        r.contains("int") && r.contains("string") && r.contains("bool"),
        "explicit A=int should win (render carries int|string|bool), got {r:?}"
    );
}

// ── §D: divergent generic-instance args union-merge (covariant) ─────────────

#[tokio::test]
async fn infer_choose_divergent_generic_instances_union() {
    // D2: choose(GenericBox[int], GenericBox[str]) ⇒ T = GenericBox<int> |
    // GenericBox<string> — the union forms OUTSIDE the box (both occurrences
    // covariant bare args). Contrast `combine`, where T is INSIDE the box and the
    // same actuals conflict (§J E3).
    let src = r#"
        class GenericBox<T> { value T }
        function choose<T>(left: T, right: T) -> string { reflect.type_of<T>().to_string() }
        function main() -> int { 0 }
    "#;
    let x = BexExternalValue::instance_generic(
        "GenericBox",
        vec![baml_type::RuntimeTy::int()],
        indexmap::IndexMap::from_iter([("value", BexExternalValue::Int(1))]),
    );
    let y = BexExternalValue::instance_generic(
        "GenericBox",
        vec![baml_type::RuntimeTy::string()],
        indexmap::IndexMap::from_iter([("value", s("x"))]),
    );
    let out = call_infer(src, "choose", vec![x, y]).await.unwrap();
    let BexExternalValue::String(rendered) = &out else {
        panic!("expected string, got {out:?}");
    };
    let r = rendered.as_str();
    assert!(
        r.contains("GenericBox") && r.contains("int") && r.contains("string"),
        "expected GenericBox<int> | GenericBox<string>, got {r:?}"
    );
}

// ── §H: union-with-concrete-sibling routes a generic instance to T ──────────

#[tokio::test]
async fn infer_tag_or_value_binds_generic_instance() {
    // H2: tag_or_value(GenericBox[str]) ⇒ the instance is not absorbed by the
    // `string`/`null` siblings, so it routes to T ⇒ T = GenericBox<string>.
    let src = r#"
        class GenericBox<T> { value T }
        function tag_or_value<T>(x: T | string | null) -> string {
            reflect.type_of<T>().to_string()
        }
        function main() -> int { 0 }
    "#;
    let arg = BexExternalValue::instance_generic(
        "GenericBox",
        vec![baml_type::RuntimeTy::string()],
        indexmap::IndexMap::from_iter([("value", s("asdf"))]),
    );
    let out = call_infer(src, "tag_or_value", vec![arg]).await.unwrap();
    let BexExternalValue::String(rendered) = &out else {
        panic!("expected string, got {out:?}");
    };
    let r = rendered.as_str();
    assert!(
        r.contains("GenericBox") && r.contains("string"),
        "expected T = GenericBox<string>, got {r:?}"
    );
}

// ── §I: enum actual widens to the enum type ─────────────────────────────────

#[tokio::test]
async fn infer_identity_enum_binds_enum_type() {
    // I3: identity(SomeEnum.VARIANT) ⇒ T = SomeEnum (the enum type, recovered
    // from a resolved variant value).
    let src = r#"
        enum SomeEnum { VARIANT OTHER }
        function identity<T>(x: T) -> string { reflect.type_of<T>().to_string() }
        function main() -> int { 0 }
    "#;
    let arg = BexExternalValue::Variant {
        enum_name: "SomeEnum".to_string(),
        variant_name: "VARIANT".to_string(),
    };
    let out = call_infer(src, "identity", vec![arg]).await.unwrap();
    let BexExternalValue::String(rendered) = &out else {
        panic!("expected string, got {out:?}");
    };
    assert!(
        rendered.as_str().contains("SomeEnum"),
        "expected T = SomeEnum, got {rendered:?}"
    );
}
