//! Inbound generics *inference* (01a/01b): a bare generic call (empty
//! `type_args`) has its `TypeVar`s solved from the argument *values* by the
//! engine, then handed to the unchanged explicit downstream. Mirrors the
//! `call_named` harness of `generics_runtime.rs` but passes EMPTY bindings and
//! asserts inference filled them. Case labels map to `00b3-labeled-cases.md`.

mod common;

use std::sync::Arc;

use baml_type::RuntimeTy;
use bex_engine::{BexEngine, BexExternalValue as BEV, EngineError, FunctionCallContextBuilder};
use bex_resource_types::{HostValueArc, HostValueKind};
use common::compile_for_engine;
use indexmap::{IndexMap, indexmap};
use sys_native::SysOpsExt;

/// Call `function` with the given argument values and NO explicit type bindings
/// — the engine must infer every `TypeVar` from the values.
async fn call_infer(source: &str, function: &str, args: Vec<BEV>) -> Result<BEV, EngineError> {
    call_with_bindings(source, function, args, IndexMap::new()).await
}

/// Like `call_infer` but seeds some explicit bindings (for the partial-binding
/// and explicit-wins cases).
async fn call_with_bindings(
    source: &str,
    function: &str,
    args: Vec<BEV>,
    type_args: IndexMap<&str, RuntimeTy>,
) -> Result<BEV, EngineError> {
    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );
    let type_args = type_args
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    engine
        .call_function(
            function,
            args,
            FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_type_args(type_args)
                .build(),
            true,
        )
        .await
}

fn s(v: &str) -> BEV {
    BEV::String(v.into())
}

/// A free generic function reflecting its single `TypeVar` as a string — the
/// observable proof of what `T` was bound to.
const IDENTITY: &str = r#"
    function identity<T>(x: T) -> string { type.of<T>().to_string() }
    function main() -> int { 0 }
"#;

// ── §A: known-type args ────────────────────────────────────────────────────

#[tokio::test]
async fn infer_identity_int() {
    // T1: identity(5) ⇒ T=int.
    let out = call_infer(IDENTITY, "identity", vec![BEV::Int(5)])
        .await
        .unwrap();
    assert_eq!(out, BEV::String("int".into()));
}

#[tokio::test]
async fn infer_identity_string_widens() {
    // T2/T45: identity("hi") ⇒ T=string (widened, never a literal).
    let out = call_infer(IDENTITY, "identity", vec![s("hi")])
        .await
        .unwrap();
    assert_eq!(out, BEV::String("string".into()));
}

#[tokio::test]
async fn infer_identity_bool() {
    let out = call_infer(IDENTITY, "identity", vec![BEV::Bool(true)])
        .await
        .unwrap();
    assert_eq!(out, BEV::String("bool".into()));
}

#[tokio::test]
async fn infer_identity_null_binds_rust_type() {
    // §I I4 (decided): a bare `null` actual gives the value position no concrete
    // leaf, so we do NOT bind `T = null`; `T` defaults to host-only `rust_type`
    // (rule 4) and the value round-trips unchanged.
    let out = call_infer(IDENTITY, "identity", vec![BEV::Null])
        .await
        .unwrap();
    assert_eq!(out, BEV::String("$rust_type".into()));
}

// ── §A: generic instance arg carries wire type_args ────────────────────────

#[tokio::test]
async fn infer_identity_generic_instance() {
    // T4-ish: a fully-bound GenericBox[int] instance ⇒ T = GenericBox<int>.
    let source = r#"
        class GenericBox<T> { value T }
        function identity<T>(x: T) -> string { type.of<T>().to_string() }
        function main() -> int { 0 }
    "#;
    let arg = BEV::instance_generic(
        "GenericBox",
        vec![RuntimeTy::int()],
        indexmap::IndexMap::from_iter([("value", BEV::Int(5))]),
    );
    let out = call_infer(source, "identity", vec![arg]).await.unwrap();
    // Exact render of the class type is determined by RuntimeTy Display; assert
    // it mentions the class and its int arg.
    let BEV::String(rendered) = &out else {
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
            type.of<A | B | C>().to_string()
        }
        function main() -> int { 0 }
    "#;
    let b = BEV::Array {
        element_type: RuntimeTy::string(),
        items: vec![s("a"), s("b")],
    };
    let c = BEV::Map {
        key_type: RuntimeTy::string(),
        value_type: RuntimeTy::bool(),
        entries: indexmap::IndexMap::from_iter([("k".to_string(), BEV::Bool(true))]),
    };
    let out = call_infer(source, "make_triple", vec![BEV::Int(1), b, c])
        .await
        .unwrap();
    let BEV::String(rendered) = &out else {
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
        function second_of<T>(p: GenericPair<int, T>) -> string { type.of<T>().to_string() }
        function main() -> int { 0 }
    "#;
    let arg = BEV::instance_generic(
        "GenericPair",
        vec![RuntimeTy::int(), RuntimeTy::string()],
        indexmap::IndexMap::from_iter([("first", BEV::Int(1)), ("second", s("hi"))]),
    );
    let out = call_infer(source, "second_of", vec![arg]).await.unwrap();
    assert_eq!(out, BEV::String("string".into()));
}

// ── §C: union merge (same var, multiple positions) ─────────────────────────

const CHOOSE: &str = r#"
    function choose<T>(left: T, right: T) -> string { type.of<T>().to_string() }
    function main() -> int { 0 }
"#;

#[tokio::test]
async fn infer_choose_same_type_merges_to_one() {
    // T14: choose(5, 6) ⇒ T = int (union(int,int) dedups).
    let out = call_infer(CHOOSE, "choose", vec![BEV::Int(5), BEV::Int(6)])
        .await
        .unwrap();
    assert_eq!(out, BEV::String("int".into()));
}

#[tokio::test]
async fn infer_choose_divergent_unions() {
    // T15: choose(5, "a") ⇒ T = int | string. Assert the render mentions both.
    let out = call_infer(CHOOSE, "choose", vec![BEV::Int(5), s("a")])
        .await
        .unwrap();
    let BEV::String(rendered) = &out else {
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
    // C2/T17: make_triple with A explicitly seeded, B/C inferred. The seed must be
    // consistent with the actual (post-C4 a contradicting seed rejects — see
    // `explicit_binding_contradicted_by_scalar_actual_rejects`), so we seed a
    // WIDER type the value still inhabits: A = `int | float` against an int(1)
    // actual. `float` appears in the render ONLY because the explicit seed shaped
    // A — inference alone would bind A=int — proving the partial seed is honored
    // while B=string and C=bool are still inferred.
    let source = r#"
        function make_triple<A, B, C>(a: A, b: B[], c: map<string, C>) -> string {
            type.of<A | B | C>().to_string()
        }
        function main() -> int { 0 }
    "#;
    let b = BEV::Array {
        element_type: RuntimeTy::string(),
        items: vec![s("x")],
    };
    let c = BEV::Map {
        key_type: RuntimeTy::string(),
        value_type: RuntimeTy::bool(),
        entries: indexmap::IndexMap::from_iter([("k".to_string(), BEV::Bool(true))]),
    };
    let out = call_with_bindings(
        source,
        "make_triple",
        vec![BEV::Int(1), b, c],
        indexmap! {
            "A" => RuntimeTy::Union(
                vec![RuntimeTy::int(), RuntimeTy::float()],
                baml_type::TyAttr::default(),
            )
        },
    )
    .await
    .unwrap();
    let BEV::String(rendered) = &out else {
        panic!("expected string, got {out:?}");
    };
    let r = rendered.as_str();
    assert!(
        r.contains("float") && r.contains("string") && r.contains("bool"),
        "explicit A=int|float should be honored (render carries float); got {r:?}"
    );
}

#[tokio::test]
async fn explicit_binding_widens_inferred_type() {
    // An explicit binding overrides what inference would pick, but it must still
    // be inhabited by the actual (post-C4). `identity(5)` with explicit
    // `T = int | string` renders the wider `int | string` — inference alone would
    // bind T=int — and the int(5) actual satisfies the wider union. (A
    // *contradicting* explicit binding, e.g. T=string for an int actual, now
    // rejects: see `explicit_binding_contradicted_by_scalar_actual_rejects`.)
    let out = call_with_bindings(
        IDENTITY,
        "identity",
        vec![BEV::Int(5)],
        indexmap! {
            "T" => RuntimeTy::Union(
                vec![RuntimeTy::int(), RuntimeTy::string()],
                baml_type::TyAttr::default(),
            )
        },
    )
    .await
    .unwrap();
    let BEV::String(rendered) = &out else {
        panic!("expected string, got {out:?}");
    };
    assert!(
        rendered.as_str().contains("int") && rendered.as_str().contains("string"),
        "explicit T=int|string should render the wider union, got {rendered:?}"
    );
}

// ── §E: must-specify still rejects (return/body-only vars) ─────────────────

#[tokio::test]
async fn body_only_var_still_requires_binding() {
    // T19: one_type_arg() with no value carrying T ⇒ Gate A rejects.
    let source = r#"
        function one_type_arg<T>() -> string { type.of<T>().to_string() }
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
fn host_only() -> BEV {
    BEV::RustData(Arc::new(()))
}

#[tokio::test]
async fn infer_identity_host_only_binds_rust_type() {
    // T24: identity(host_obj) ⇒ T = rust_type; reflect renders the rust type.
    let out = call_infer(IDENTITY, "identity", vec![host_only()])
        .await
        .unwrap();
    let BEV::String(rendered) = &out else {
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
    let out = call_infer(CHOOSE, "choose", vec![BEV::Int(5), host_only()])
        .await
        .unwrap();
    let BEV::String(rendered) = &out else {
        panic!("expected string, got {out:?}");
    };
    assert!(
        rendered.as_str().contains("int") && rendered.as_str().contains("rust"),
        "expected int|rust_type render, got {rendered:?}"
    );
}

#[tokio::test]
async fn f2_wrap_host_only_returns_generic_box_of_rust_type() {
    // §F F2: wrap(host_obj) ⇒ T = rust_type; returns a `GenericBox<rust_type>`
    // wrapping the opaque handle (the box materializes, its element rides as
    // rust_data).
    let source = r#"
        class GenericBox<T> { value T }
        function wrap<T>(x: T) -> GenericBox<T> { GenericBox<T> { value: x } }
        function main() -> int { 0 }
    "#;
    let out = call_infer(source, "wrap", vec![host_only()]).await.unwrap();
    // The result is a `GenericBox` instance (its `value` field is the opaque
    // handle, round-tripped as rust_data).
    assert!(
        matches!(&out, BEV::Instance { class_name, .. } if class_name.contains("GenericBox")),
        "expected a GenericBox instance, got {out:?}"
    );
}

#[tokio::test]
async fn g3_nested_unbound_under_bare_formal_is_rust_type() {
    // §G G3: identity(GenericBox(value=GenericBox(value="hello"))) — the OUTER
    // instance is unbound under a bare-`T` formal ⇒ the whole value is rust_type.
    let source = r#"
        class GenericBox<T> { value T }
        function identity<T>(x: T) -> string { type.of<T>().to_string() }
        function main() -> int { 0 }
    "#;
    let inner = BEV::instance(
        "GenericBox",
        indexmap::IndexMap::from_iter([("value", s("hello"))]),
    );
    let outer = BEV::instance(
        "GenericBox",
        indexmap::IndexMap::from_iter([("value", inner)]),
    );
    let out = call_infer(source, "identity", vec![outer]).await.unwrap();
    assert_eq!(out, BEV::String("$rust_type".into()));
}

// ── §J J13: a closure-typed parameter poisons its TypeVars (must-specify) ────

#[tokio::test]
async fn j13_closure_typed_param_poisons_typevars_must_specify() {
    // §J J13: apply<T,R>(f: (T) -> R, x: T) — `f` is a host callable, opaque to
    // BAML, so `T` and `R` are poisoned and must be specified explicitly EVEN
    // THOUGH `x` would otherwise pin `T=int`. With no explicit bindings the call
    // is rejected (must-specify), not silently inferred.
    let source = r#"
        function apply<T, R>(f: (T) -> R, x: T) -> string {
            type.of<T>().to_string()
        }
        function main() -> int { 0 }
    "#;
    let f = BEV::HostValue(HostValueArc::new(1, HostValueKind::Callable));
    let err = call_infer(source, "apply", vec![f, BEV::Int(5)])
        .await
        .expect_err("T and R are closure-poisoned ⇒ must be specified explicitly");
    assert!(
        matches!(err, EngineError::TypeMismatch { .. }),
        "expected a must-specify TypeMismatch, got {err:?}"
    );
}

#[tokio::test]
async fn j13_closure_poisoned_typevars_succeed_when_specified() {
    // §J J13 (positive): the same call succeeds once `T` and `R` are specified.
    let source = r#"
        function apply<T, R>(f: (T) -> R, x: T) -> string {
            type.of<T>().to_string()
        }
        function main() -> int { 0 }
    "#;
    let f = BEV::HostValue(HostValueArc::new(2, HostValueKind::Callable));
    let out = call_with_bindings(
        source,
        "apply",
        vec![f, BEV::Int(5)],
        indexmap! {
            "T" => RuntimeTy::int(),
            "R" => RuntimeTy::int(),
        },
    )
    .await
    .unwrap();
    assert_eq!(out, BEV::String("int".into()));
}

// ── §J: empty collections → rust_type element/value ────────────────────────

#[tokio::test]
async fn infer_identity_empty_array_binds_rust_type_list() {
    // An empty `[]` carries no element evidence, but it still inhabits a list:
    // `identity([])` ⇒ T = rust_type[], NOT a Gate-A rejection.
    let arg = BEV::Array {
        element_type: RuntimeTy::int(),
        items: vec![],
    };
    let out = call_infer(IDENTITY, "identity", vec![arg]).await.unwrap();
    let BEV::String(rendered) = &out else {
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
    let arg = BEV::Map {
        key_type: RuntimeTy::string(),
        value_type: RuntimeTy::int(),
        entries: indexmap::IndexMap::new(),
    };
    let out = call_infer(IDENTITY, "identity", vec![arg]).await.unwrap();
    let BEV::String(rendered) = &out else {
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
    function maybe_id<T>(x: T?) -> string { type.of<T>().to_string() }
    function main() -> int { 0 }
"#;

#[tokio::test]
async fn infer_maybe_id_present_value() {
    // T32: maybe_id(5) null-strips T? to bare T ⇒ T=int.
    let out = call_infer(MAYBE_ID, "maybe_id", vec![BEV::Int(5)])
        .await
        .unwrap();
    assert_eq!(out, BEV::String("int".into()));
}

#[tokio::test]
async fn infer_maybe_id_null_value() {
    // §I I4 (decided): maybe_id(None) does NOT null-strip `T?` to bind `T=null`;
    // a `null`-only actual is no evidence ⇒ `T` defaults to `rust_type`.
    let out = call_infer(MAYBE_ID, "maybe_id", vec![BEV::Null])
        .await
        .unwrap();
    assert_eq!(out, BEV::String("$rust_type".into()));
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
        type.of<T>().to_string()
    }
    function main() -> int { 0 }
"#;

#[tokio::test]
async fn infer_tag_or_value_binds_t_from_int() {
    // T30: tag_or_value(5) ⇒ T=int. The int is not absorbed by the `string`/
    // `null` siblings, so it routes to the lone `TypeVar` member.
    let out = call_infer(TAG_OR_VALUE_REFLECT, "tag_or_value", vec![BEV::Int(5)])
        .await
        .unwrap();
    assert_eq!(out, BEV::String("int".into()));
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
    assert_eq!(out, BEV::String("$rust_type".into()));
}

#[tokio::test]
async fn infer_tag_or_value_null_binds_rust_type() {
    // §H H3 (decided): tag_or_value(None) ⇒ a `null` actual is no evidence (it is
    // not bound as `T=null`), and the `null` sibling absorbs it, so `T` gets no
    // residual ⇒ defaults to `rust_type` (rule 4).
    let out = call_infer(TAG_OR_VALUE_REFLECT, "tag_or_value", vec![BEV::Null])
        .await
        .unwrap();
    assert_eq!(out, BEV::String("$rust_type".into()));
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
    let out = call_infer(TAG_OR_VALUE_MATCH, "tag_or_value", vec![BEV::Int(5)])
        .await
        .unwrap();
    assert_eq!(out, BEV::Int(5));
}

// ── §F: leaf ADT synth (media / collector) ─────────────────────────────────

#[tokio::test]
async fn infer_identity_media_binds_concrete_media_type() {
    // Regression: a media argument is a concrete leaf BAML type, so
    // identity<T>(image) must bind T = image (not the host-only rust_type that
    // the Adt(_) catch-all previously synthesized).
    use bex_external_types::{BexExternalAdt, MediaKind};
    let media = baml_builtins2::MediaValue::from_url(
        MediaKind::Image,
        "http://example.com/y.png",
        Some("image/png"),
    );
    let arg = BEV::Adt(BexExternalAdt::Media(media));
    let out = call_infer(IDENTITY, "identity", vec![arg]).await.unwrap();
    assert_eq!(out, BEV::String("image".into()));
}

#[tokio::test]
#[ignore = "Declares `map<K, int[]>` (a type-variable map key), now forbidden — map keys must be `string` (a typevar could bind to a non-string). Un-ignore only if bounded string-keyed generic maps are supported."]
async fn infer_nonempty_map_binds_key_despite_valueless_values() {
    // Regression: a NON-empty map whose values give no evidence (an empty array)
    // still carries key evidence — every wire entry is string-keyed — so the
    // map-key TypeVar K must bind to `string` rather than collapsing the whole
    // map to NoEvidence (which previously left K unbound and Gate-A rejected).
    let src = r#"
        function map_key<K>(m: map<K, int[]>) -> string { type.of<K>().to_string() }
        function main() -> int { 0 }
    "#;
    let mut entries = indexmap::IndexMap::new();
    entries.insert(
        "a".to_string(),
        BEV::Array {
            element_type: RuntimeTy::int(),
            items: vec![],
        },
    );
    let arg = BEV::Map {
        key_type: RuntimeTy::string(),
        value_type: RuntimeTy::List(Box::new(RuntimeTy::int()), baml_type::TyAttr::default()),
        entries,
    };
    let out = call_infer(src, "map_key", vec![arg]).await.unwrap();
    assert_eq!(out, BEV::String("string".into()));
}

// ── §H/§E rule 3: class-method body-only own TypeVar must-specify ───────────

#[tokio::test]
async fn gatea_class_method_body_only_var_should_reject() {
    // Rule 3 (no value position ⇒ must-specify) for a class method's OWN var:
    // `reflect_t<T>()` reflects `T` but `T` is body-only (never in a param or the
    // return), so inference has no evidence and the caller must specify it. Gate A
    // now splits a method's own params (the suffix after the class prefix, via the
    // class's `generic_param_count`) from inherited class params and demands the
    // own ones — so this rejects, matching the free-function analogue
    // (`body_only_var_still_requires_binding`). Previously check(1) was skipped for
    // ALL class methods and `T` silently erased to `unknown`.
    let src = r#"
        class Helper {
            function reflect_t<T>() -> string { type.of<T>().to_string() }
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
async fn unbound_generic_instance_should_be_host_only() {
    // §G G2 (decided): an `Instance` of a *generic* class with EMPTY wire
    // type_args is an unbound generic ⇒ host-only `rust_type`, NOT a fabricated
    // `GenericBox<>`. The bare-`T` formal gives inference nothing to descend
    // into, so `T` defaults to `rust_type`.
    let source = r#"
        class GenericBox<T> { value T }
        function identity<T>(x: T) -> string { type.of<T>().to_string() }
        function main() -> int { 0 }
    "#;
    // Instance whose wire type_args are EMPTY (unbound generic class encoding).
    let arg = BEV::instance(
        "GenericBox",
        indexmap::IndexMap::from_iter([("value", BEV::Int(5))]),
    );
    let out = call_infer(source, "identity", vec![arg]).await.unwrap();
    assert_eq!(out, BEV::String("$rust_type".into()));
}

/// A value-returning identity — proves a value *round-trips* (not just that `T`
/// reflects), so it exercises the host-only `OpaqueExternalValue` carrier.
const IDENTITY_VALUE: &str = r#"
    class GenericBox<T> { value T }
    function identity<T>(x: T) -> T { x }
    function main() -> int { 0 }
"#;

#[tokio::test]
async fn g2_unbound_generic_instance_round_trips_opaque() {
    // §G G2: `identity(GenericBox(value=5))` over an UNBOUND instance ⇒ T=rust_type;
    // the instance rides through the VM opaquely and comes back verbatim.
    let unbound = BEV::instance(
        "GenericBox",
        indexmap::IndexMap::from_iter([("value", BEV::Int(5))]),
    );
    let out = call_infer(IDENTITY_VALUE, "identity", vec![unbound.clone()])
        .await
        .unwrap();
    assert_eq!(
        out, unbound,
        "unbound generic instance must round-trip unchanged"
    );
}

#[tokio::test]
async fn g4_bound_and_unbound_generic_instances_are_distinct() {
    // §G G4 (discriminator): a properly-bound `GenericBox[int]` round-trips as a
    // real bound instance (wire args `[int]`), while the UNBOUND form rides
    // opaquely — so the two are NOT equal after a round-trip.
    let bound = BEV::instance_generic(
        "GenericBox",
        vec![RuntimeTy::int()],
        indexmap::IndexMap::from_iter([("value", BEV::Int(5))]),
    );
    let unbound = BEV::instance(
        "GenericBox",
        indexmap::IndexMap::from_iter([("value", BEV::Int(5))]),
    );
    let bound_out = call_infer(IDENTITY_VALUE, "identity", vec![bound])
        .await
        .unwrap();
    let unbound_out = call_infer(IDENTITY_VALUE, "identity", vec![unbound])
        .await
        .unwrap();
    assert_ne!(
        bound_out, unbound_out,
        "a bound `GenericBox[int]` must not equal the unbound `GenericBox(value=5)`"
    );
}

#[tokio::test]
async fn g1_unbound_instance_under_forcing_formal_recovers_field_type() {
    // §G G1: an UNBOUND `GenericPair(first=1, second="hi")` met by the forcing
    // formal `GenericPair<int, T>` recovers `T=string` from the field VALUE (the
    // wire carries no type-args), distinct from the bare-`T` host-only path.
    let source = r#"
        class GenericPair<A, B> { first A second B }
        function second_of<T>(p: GenericPair<int, T>) -> string { type.of<T>().to_string() }
        function main() -> int { 0 }
    "#;
    let unbound = BEV::instance(
        "GenericPair",
        indexmap::IndexMap::from_iter([("first", BEV::Int(1)), ("second", s("hi"))]),
    );
    let out = call_infer(source, "second_of", vec![unbound])
        .await
        .unwrap();
    assert_eq!(out, BEV::String("string".into()));
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

fn arr(element_type: RuntimeTy, items: Vec<BEV>) -> BEV {
    BEV::Array {
        element_type,
        items,
    }
}

fn int_map(value_type: RuntimeTy, entries: &[(&str, BEV)]) -> BEV {
    BEV::Map {
        key_type: RuntimeTy::string(),
        value_type,
        entries: entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    }
}

/// `pair<T>(a: T[], b: T[])` — `T` in two **invariant** (list-element) positions.
const PAIR: &str = r#"
    function pair<T>(a: T[], b: T[]) -> string { type.of<T>().to_string() }
    function main() -> int { 0 }
"#;

#[tokio::test]
async fn infer_pair_invariant_list_conflict_rejects() {
    // J4/E1: pair(int[], string[]) ⇒ a⇒T==int, b⇒T==string (both invariant) ⇒
    // no consistent T ⇒ reject (today's bug fabricated `(int|string)[]`).
    let a = arr(RuntimeTy::int(), vec![BEV::Int(1)]);
    let b = arr(RuntimeTy::string(), vec![s("x")]);
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
    let a = arr(RuntimeTy::int(), vec![BEV::Int(1)]);
    let b = arr(RuntimeTy::int(), vec![BEV::Int(2)]);
    let out = call_infer(PAIR, "pair", vec![a, b]).await.unwrap();
    assert_eq!(out, BEV::String("int".into()));
}

#[tokio::test]
async fn infer_choose_union_outside_container_joins() {
    // J10/G2 regression: choose(int[], string[]) ⇒ both occurrences are covariant
    // (bare `T`), so the union forms OUTSIDE the container ⇒ T = int[] | string[].
    // Proves the fix keys on position variance, not "arrays are involved."
    let a = arr(RuntimeTy::int(), vec![BEV::Int(1)]);
    let b = arr(RuntimeTy::string(), vec![s("x")]);
    let out = call_infer(CHOOSE, "choose", vec![a, b]).await.unwrap();
    let BEV::String(rendered) = &out else {
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
            type.of<T>().to_string()
        }
        function main() -> int { 0 }
    "#;
    let a = int_map(RuntimeTy::int(), &[("k", BEV::Int(1))]);
    let b = int_map(RuntimeTy::string(), &[("k", s("x"))]);
    let err = call_infer(src, "merge", vec![a, b])
        .await
        .expect_err("invariant map-value conflict must reject");
    assert!(
        matches!(err, EngineError::TypeMismatch { .. }),
        "got {err:?}"
    );
}

#[tokio::test]
async fn infer_combine_invariant_class_arg_conflict_rejects() {
    // J6/E3: combine(GenericBox[int], GenericBox[string]) ⇒ Box<T> invariant,
    // int ≠ string ⇒ reject.
    let src = r#"
        class GenericBox<T> { value T }
        function combine<T>(x: GenericBox<T>, y: GenericBox<T>) -> string {
            type.of<T>().to_string()
        }
        function main() -> int { 0 }
    "#;
    let x = BEV::instance_generic(
        "GenericBox",
        vec![RuntimeTy::int()],
        indexmap::IndexMap::from_iter([("value", BEV::Int(1))]),
    );
    let y = BEV::instance_generic(
        "GenericBox",
        vec![RuntimeTy::string()],
        indexmap::IndexMap::from_iter([("value", s("x"))]),
    );
    let err = call_infer(src, "combine", vec![x, y])
        .await
        .expect_err("invariant class-arg conflict must reject");
    assert!(
        matches!(err, EngineError::TypeMismatch { .. }),
        "got {err:?}"
    );
}

#[tokio::test]
async fn infer_glue_invariant_vs_covariant_conflict_rejects() {
    // J7/E4: glue(int, string[]) ⇒ arr⇒T==string (invariant) but bare⇒int <: T
    // (covariant); int <: string is false ⇒ reject (the key cross-variance case).
    let src = r#"
        function glue<T>(bare: T, arr: T[]) -> string { type.of<T>().to_string() }
        function main() -> int { 0 }
    "#;
    let arr_arg = arr(RuntimeTy::string(), vec![s("x")]);
    let err = call_infer(src, "glue", vec![BEV::Int(1), arr_arg])
        .await
        .expect_err("invariant×covariant conflict must reject");
    assert!(
        matches!(err, EngineError::TypeMismatch { .. }),
        "got {err:?}"
    );
}

#[tokio::test]
async fn infer_glue_invariant_and_covariant_agree_binds() {
    // J11/G4 regression: glue(int, int[]) ⇒ invariant (T==int) + covariant
    // (int <: int) AGREE ⇒ T = int; must succeed.
    let src = r#"
        function glue<T>(bare: T, arr: T[]) -> string { type.of<T>().to_string() }
        function main() -> int { 0 }
    "#;
    let arr_arg = arr(RuntimeTy::int(), vec![BEV::Int(2)]);
    let out = call_infer(src, "glue", vec![BEV::Int(1), arr_arg])
        .await
        .unwrap();
    assert_eq!(out, BEV::String("int".into()));
}

// ── §D: n-ary covariant join ────────────────────────────────────────────────

#[tokio::test]
async fn infer_triple_choose_three_covariant_join() {
    // D3: triple_choose(5, "asdf", True) ⇒ T = int | string | bool — three
    // covariant bare-arg occurrences union-merge (n-ary, not pairwise-special).
    let src = r#"
        function triple_choose<T>(a: T, b: T, c: T) -> string {
            type.of<T>().to_string()
        }
        function main() -> int { 0 }
    "#;
    let out = call_infer(
        src,
        "triple_choose",
        vec![BEV::Int(5), s("asdf"), BEV::Bool(true)],
    )
    .await
    .unwrap();
    let BEV::String(rendered) = &out else {
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
            type.of<A | B | C>().to_string()
        }
        function main() -> int { 0 }
    "#;
    let b = arr(RuntimeTy::int(), vec![BEV::Int(1), s("x")]);
    let c = int_map(RuntimeTy::bool(), &[("k", BEV::Bool(true))]);
    let out = call_infer(src, "make_triple", vec![BEV::Int(1), b, c])
        .await
        .unwrap();
    let BEV::String(rendered) = &out else {
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
            type.of<T | U>().to_string()
        }
        function main() -> int { 0 }
    "#;
    let err = call_infer(src, "two_in_union", vec![s("hello")])
        .await
        .expect_err("two free vars in one union are un-inferrable");
    assert!(
        matches!(err, EngineError::TypeMismatch { .. }),
        "got {err:?}"
    );
}

// ── §A: known non-generic BAML class actual ─────────────────────────────────

#[tokio::test]
async fn infer_identity_known_class_instance() {
    // A2: identity(StringIntPair(...)) ⇒ T = StringIntPair (a known, non-generic
    // BAML class recovered from the instance value).
    let src = r#"
        class StringIntPair { my_string string my_int int }
        function identity<T>(x: T) -> string { type.of<T>().to_string() }
        function main() -> int { 0 }
    "#;
    let arg = BEV::instance(
        "StringIntPair",
        indexmap::IndexMap::from_iter([("my_string", s("a")), ("my_int", BEV::Int(1))]),
    );
    let out = call_infer(src, "identity", vec![arg]).await.unwrap();
    let BEV::String(rendered) = &out else {
        panic!("expected string, got {out:?}");
    };
    assert!(
        rendered.as_str().contains("StringIntPair"),
        "expected T = StringIntPair, got {rendered:?}"
    );
}

// ── §B: nested generic-class arg, four vars zipped ──────────────────────────

fn pair_rt(first: RuntimeTy, second: RuntimeTy) -> RuntimeTy {
    RuntimeTy::Class(
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
            type.of<A | B | C | D>().to_string()
        }
        function main() -> int { 0 }
    "#;
    let inner1 = BEV::instance_generic(
        "GenericPair",
        vec![RuntimeTy::int(), RuntimeTy::string()],
        indexmap::IndexMap::from_iter([("first", BEV::Int(1)), ("second", s("a"))]),
    );
    let inner2 = BEV::instance_generic(
        "GenericPair",
        vec![RuntimeTy::bool(), RuntimeTy::float()],
        indexmap::IndexMap::from_iter([("first", BEV::Bool(true)), ("second", BEV::Float(1.5))]),
    );
    let arg = BEV::instance_generic(
        "GenericPair",
        vec![
            pair_rt(RuntimeTy::int(), RuntimeTy::string()),
            pair_rt(RuntimeTy::bool(), RuntimeTy::float()),
        ],
        indexmap::IndexMap::from_iter([("first", inner1), ("second", inner2)]),
    );
    let out = call_infer(src, "extract", vec![arg]).await.unwrap();
    let BEV::String(rendered) = &out else {
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
async fn explicit_binding_contradicted_by_scalar_actual_rejects() {
    // C4: seed A=int explicitly but pass a *string* for `a`. Inference is bypassed
    // for the caller-specified A, so the value seam's per-arg structural check
    // (Gate B) is the only gate — and it now REJECTS the string against the
    // now-concrete `int` formal. (Before the Gate B rewrite this seam skipped
    // every non-Instance arg, so the call slipped through with A=int and the
    // mismatch only surfaced host-side at decode time — the old "pins the gap"
    // case.) The friendly error names the function and both types, no jargon.
    let src = r#"
        function make_triple<A, B, C>(a: A, b: B[], c: map<string, C>) -> string {
            type.of<A | B | C>().to_string()
        }
        function main() -> int { 0 }
    "#;
    let b = BEV::Array {
        element_type: RuntimeTy::string(),
        items: vec![s("x")],
    };
    let c = BEV::Map {
        key_type: RuntimeTy::string(),
        value_type: RuntimeTy::bool(),
        entries: indexmap::IndexMap::from_iter([("k".to_string(), BEV::Bool(true))]),
    };
    let err = call_with_bindings(
        src,
        "make_triple",
        vec![s("nope"), b, c],
        indexmap! { "A" => RuntimeTy::int() },
    )
    .await
    .expect_err("a string actual must not satisfy the explicit `A=int` binding");
    let EngineError::TypeMismatch { message } = &err else {
        panic!("expected TypeMismatch, got {err:?}");
    };
    assert!(
        message.contains("make_triple") && message.contains("int") && message.contains("string"),
        "unfriendly C4 message: {message:?}"
    );
    assert!(
        !message.contains("<:") && !message.to_lowercase().contains("variance"),
        "message leaks type-theory jargon: {message:?}"
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
        function choose<T>(left: T, right: T) -> string { type.of<T>().to_string() }
        function main() -> int { 0 }
    "#;
    let x = BEV::instance_generic(
        "GenericBox",
        vec![RuntimeTy::int()],
        indexmap::IndexMap::from_iter([("value", BEV::Int(1))]),
    );
    let y = BEV::instance_generic(
        "GenericBox",
        vec![RuntimeTy::string()],
        indexmap::IndexMap::from_iter([("value", s("x"))]),
    );
    let out = call_infer(src, "choose", vec![x, y]).await.unwrap();
    let BEV::String(rendered) = &out else {
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
            type.of<T>().to_string()
        }
        function main() -> int { 0 }
    "#;
    let arg = BEV::instance_generic(
        "GenericBox",
        vec![RuntimeTy::string()],
        indexmap::IndexMap::from_iter([("value", s("asdf"))]),
    );
    let out = call_infer(src, "tag_or_value", vec![arg]).await.unwrap();
    let BEV::String(rendered) = &out else {
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
        function identity<T>(x: T) -> string { type.of<T>().to_string() }
        function main() -> int { 0 }
    "#;
    let arg = BEV::Variant {
        enum_name: "SomeEnum".to_string(),
        variant_name: "VARIANT".to_string(),
    };
    let out = call_infer(src, "identity", vec![arg]).await.unwrap();
    let BEV::String(rendered) = &out else {
        panic!("expected string, got {out:?}");
    };
    assert!(
        rendered.as_str().contains("SomeEnum"),
        "expected T = SomeEnum, got {rendered:?}"
    );
}

// ── §A A3: nested *fully-bound* generic instance ────────────────────────────

/// Build a `RuntimeTy` for `GenericBox<inner>` (the value-level wire type).
fn box_rt(inner: RuntimeTy) -> RuntimeTy {
    RuntimeTy::Class(
        baml_type::TypeName::local(baml_type::Name::new("GenericBox")),
        vec![inner],
        baml_type::TyAttr::default(),
    )
}

#[tokio::test]
async fn a3_nested_fully_bound_generic_instance() {
    // §A A3: identity over a *fully-bound* nested `GenericBox<GenericBox<string>>`
    // — every param concrete on the wire ⇒ T = GenericBox<GenericBox<string>>;
    // the render mentions the class twice and the inner `string`. (Contrast
    // `infer_identity_generic_instance`, the single-level bound box.)
    let source = r#"
        class GenericBox<T> { value T }
        function identity<T>(x: T) -> string { type.of<T>().to_string() }
        function main() -> int { 0 }
    "#;
    let inner = BEV::instance_generic(
        "GenericBox",
        vec![RuntimeTy::string()],
        indexmap::IndexMap::from_iter([("value", s("hello"))]),
    );
    let outer = BEV::instance_generic(
        "GenericBox",
        vec![box_rt(RuntimeTy::string())],
        indexmap::IndexMap::from_iter([("value", inner)]),
    );
    let out = call_infer(source, "identity", vec![outer]).await.unwrap();
    let BEV::String(rendered) = &out else {
        panic!("expected string, got {out:?}");
    };
    let r = rendered.as_str();
    assert!(
        r.matches("GenericBox").count() >= 2 && r.contains("string"),
        "expected nested GenericBox<GenericBox<string>>, got {r:?}"
    );
}

// ── §B B3/B6: instance wire-arg recovery (incl. empty fields) ───────────────

/// `read_t<T>(shape: ContainerShapes<T>)` reflects `T` so a test can read what
/// the instance's single wire type-arg bound to — WITHOUT re-unifying the
/// individual `item`/`items`/`by_key` field values.
const READ_T: &str = r#"
    class ContainerShapes<T> {
      item T
      items T[]
      by_key map<string, T>
      maybe T?
      mixed T | string | null
    }
    function read_t<T>(shape: ContainerShapes<T>) -> string {
        type.of<T>().to_string()
    }
    function main() -> int { 0 }
"#;

#[tokio::test]
async fn b3_read_items_from_instance_wire_arg() {
    // §B B3: ContainerShapes[int] — T recovered from the instance's single wire
    // type-arg, NOT by re-unifying every field.
    let shape = BEV::instance_generic(
        "ContainerShapes",
        vec![RuntimeTy::int()],
        indexmap::IndexMap::from_iter([
            ("item", BEV::Int(1)),
            (
                "items",
                arr(
                    RuntimeTy::int(),
                    vec![BEV::Int(1), BEV::Int(2), BEV::Int(3)],
                ),
            ),
            ("by_key", int_map(RuntimeTy::int(), &[("k", BEV::Int(4))])),
            ("maybe", BEV::Null),
            ("mixed", BEV::Null),
        ]),
    );
    let out = call_infer(READ_T, "read_t", vec![shape]).await.unwrap();
    assert_eq!(out, BEV::String("int".into()));
}

#[tokio::test]
async fn b6_read_items_empty_fields_bound_instance_keeps_wire_arg() {
    // §B B6: every collection field is empty, but T is still recovered from the
    // instance's wire type-arg [int] — binding is wire-arg-driven, NOT
    // synthesized from the (empty) field values. Contrast B7 (free function).
    let shape = BEV::instance_generic(
        "ContainerShapes",
        vec![RuntimeTy::int()],
        indexmap::IndexMap::from_iter([
            ("item", BEV::Int(1)),
            ("items", arr(RuntimeTy::int(), vec![])),
            ("by_key", int_map(RuntimeTy::int(), &[])),
            ("maybe", BEV::Null),
            ("mixed", BEV::Null),
        ]),
    );
    let out = call_infer(READ_T, "read_t", vec![shape]).await.unwrap();
    assert_eq!(out, BEV::String("int".into()));
}

// ── §B B4: recursive generic instance ───────────────────────────────────────

#[tokio::test]
async fn b4_list_head_recursive_generic_wire_arg() {
    // §B B4: GenericRecursive[int] bottoms out at next=None; T binds from the
    // wire type-arg.
    let src = r#"
        class GenericRecursive<T> { value T next GenericRecursive<T>? }
        function list_head_t<T>(list: GenericRecursive<T>) -> string {
            type.of<T>().to_string()
        }
        function main() -> int { 0 }
    "#;
    let tail = BEV::instance_generic(
        "GenericRecursive",
        vec![RuntimeTy::int()],
        indexmap::IndexMap::from_iter([("value", BEV::Int(8)), ("next", BEV::Null)]),
    );
    let head = BEV::instance_generic(
        "GenericRecursive",
        vec![RuntimeTy::int()],
        indexmap::IndexMap::from_iter([("value", BEV::Int(7)), ("next", tail)]),
    );
    let out = call_infer(src, "list_head_t", vec![head]).await.unwrap();
    assert_eq!(out, BEV::String("int".into()));
}

// ── §B B7/B9: empty collection on a *free function* ⇒ element T = rust_type ──

#[tokio::test]
async fn b7_first_or_empty_list_free_fn_binds_rust_type() {
    // §B B7: a free function has no wire-arg channel and an empty list yields no
    // element evidence ⇒ the *element* T = rust_type (NOT rust_type[]; that is
    // identity([]), where T is the whole list). The B6/B7 split is the point.
    let src = r#"
        function first_or<T>(xs: T[]) -> string { type.of<T>().to_string() }
        function main() -> int { 0 }
    "#;
    let out = call_infer(src, "first_or", vec![arr(RuntimeTy::int(), vec![])])
        .await
        .unwrap();
    let BEV::String(rendered) = &out else {
        panic!("expected string, got {out:?}");
    };
    let r = rendered.as_str();
    assert!(
        r.contains("rust") && !r.contains("[]"),
        "expected element T = rust_type, got {r:?}"
    );
}

#[tokio::test]
async fn b9_values_of_empty_map_free_fn_binds_rust_type() {
    // §B B9: the map-value position is the only evidence channel and the empty
    // map yields no value ⇒ T = rust_type (the empty-collection rule applies to
    // `map<_,T>` just as B7 shows for `T[]`).
    let src = r#"
        function values_of<T>(m: map<string, T>) -> string {
            type.of<T>().to_string()
        }
        function main() -> int { 0 }
    "#;
    let out = call_infer(src, "values_of", vec![int_map(RuntimeTy::int(), &[])])
        .await
        .unwrap();
    let BEV::String(rendered) = &out else {
        panic!("expected string, got {out:?}");
    };
    let r = rendered.as_str();
    assert!(
        r.contains("rust") && !r.contains("map"),
        "expected value T = rust_type, got {r:?}"
    );
}

// ── §L: methods — class T from the receiver, method vars from method args ────

/// `GenericBox<T>` with the §L method set: `get` (class `T` only), `pair_with`
/// (class `T` + method `U`), and the static `new` (its own `V`).
const GENERIC_BOX_METHODS: &str = r#"
    class GenericBox<T> {
      value T
      function get(self) -> string { type.of<T>().to_string() }
      function pair_with<U>(self, other: U) -> string {
          type.of<T | U>().to_string()
      }
      function new<V>(value: V) -> GenericBox<V> { GenericBox<V> { value: value } }
    }
    function main() -> int { 0 }
"#;

#[tokio::test]
async fn l1_method_class_var_from_receiver() {
    // §L L1: GenericBox[int](value=5).get() == "int" — class T recovered from the
    // receiver's wire type-args.
    let recv = BEV::instance_generic(
        "GenericBox",
        vec![RuntimeTy::int()],
        indexmap::IndexMap::from_iter([("value", BEV::Int(5))]),
    );
    let out = call_infer(GENERIC_BOX_METHODS, "GenericBox.get", vec![recv])
        .await
        .unwrap();
    assert_eq!(out, BEV::String("int".into()));
}

#[tokio::test]
async fn l2_method_class_and_method_vars() {
    // §L L2: GenericBox[int](value=5).pair_with("hi") == "int | string" — class
    // T=int from the receiver, method U=string inferred from `other`.
    let recv = BEV::instance_generic(
        "GenericBox",
        vec![RuntimeTy::int()],
        indexmap::IndexMap::from_iter([("value", BEV::Int(5))]),
    );
    let out = call_infer(
        GENERIC_BOX_METHODS,
        "GenericBox.pair_with",
        vec![recv, s("hi")],
    )
    .await
    .unwrap();
    let BEV::String(r) = &out else {
        panic!("expected string, got {out:?}");
    };
    assert!(
        r.as_str().contains("int") && r.as_str().contains("string"),
        "expected `int | string`, got {r:?}"
    );
}

#[tokio::test]
async fn l3_static_method_infers_own_var() {
    // §L L3: GenericBox.new(value=5) — static method, V=int inferred from `value`
    // (no subscript); returns a GenericBox carrying value 5. The class var T has
    // no occurrence in the call's signature, so it is simply not required.
    let out = call_infer(GENERIC_BOX_METHODS, "GenericBox.new", vec![BEV::Int(5)])
        .await
        .unwrap();
    let BEV::Instance {
        class_name, fields, ..
    } = &out
    else {
        panic!("expected a GenericBox instance, got {out:?}");
    };
    assert!(class_name.contains("GenericBox"), "got {class_name:?}");
    assert_eq!(fields.get("value"), Some(&BEV::Int(5)));
}

#[tokio::test]
async fn l4_named_static_distinct_method_vars() {
    // §L L4: NamedStatic.make(1, "x") == "int | string" — distinct method var
    // names D=int, E=string from the args.
    let src = r#"
        class NamedStatic<A, B, C> {
          first A
          second B
          third C
          function make<D, E>(d: D, e: E) -> string {
              type.of<D | E>().to_string()
          }
        }
        function main() -> int { 0 }
    "#;
    let out = call_infer(src, "NamedStatic.make", vec![BEV::Int(1), s("x")])
        .await
        .unwrap();
    let BEV::String(r) = &out else {
        panic!("expected string, got {out:?}");
    };
    assert!(
        r.as_str().contains("int") && r.as_str().contains("string"),
        "expected `int | string`, got {r:?}"
    );
}

#[tokio::test]
async fn l5_unbound_receiver_method_recovers_class_var_from_field() {
    // §L L5 (pins ACTUAL behavior): an UNBOUND `GenericBox(value=5)` receiver
    // (no `[...]`) carries no wire type-args, but the method's `self: GenericBox<T>`
    // formal FORCES recursion into the `value` field — exactly the G1 forcing-formal
    // path — so the class `T` is recovered from the field VALUE (`5` ⇒ int), NOT
    // left as host-only rust_type. `type.of<T | U>` then renders
    // `int | string` (U=string from `other`). (The 03b L5 sketch guessed rust_type
    // under uncertainty and told us to assert whatever the implementation renders;
    // the forcing-formal recovery wins, which is the sounder outcome.)
    let recv = BEV::instance(
        "GenericBox",
        indexmap::IndexMap::from_iter([("value", BEV::Int(5))]),
    );
    let out = call_infer(
        GENERIC_BOX_METHODS,
        "GenericBox.pair_with",
        vec![recv, s("x")],
    )
    .await
    .unwrap();
    let BEV::String(r) = &out else {
        panic!("expected string, got {out:?}");
    };
    assert!(
        r.as_str().contains("int") && r.as_str().contains("string"),
        "expected `int | string` (class T recovered from the field), got {r:?}"
    );
}

// ── §B/§D: heterogeneous array unifies its element type ──────────────────────

#[tokio::test]
async fn het_array_element_type_unifies() {
    // The elements of a single `T[]` union-merge while synthesizing the
    // container's element type ⇒ elem_type([1, "x"]) binds T = int | string.
    // Directly asserts the unified element type (distinct from B8, which reads
    // the union via make_triple's `B[]`).
    let src = r#"
        function elem_type<T>(xs: T[]) -> string { type.of<T>().to_string() }
        function main() -> int { 0 }
    "#;
    let xs = arr(RuntimeTy::int(), vec![BEV::Int(1), s("x")]);
    let out = call_infer(src, "elem_type", vec![xs]).await.unwrap();
    let BEV::String(r) = &out else {
        panic!("expected string, got {out:?}");
    };
    assert!(
        r.as_str().contains("int") && r.as_str().contains("string"),
        "expected unified `int | string`, got {r:?}"
    );
}

// ── §G generalized: UNBOUND instances recovered via a forcing formal ─────────
//
// An unbound generic instance (empty wire type-args) normally rides as host-only
// `rust_type` (G2). But when the parameter's formal is `Container<T>` /
// `Recursive<T>` / nested `Pair<…>`, inference is DIRECTED into the field values
// and recovers `T` from them (G1) — so even a wire with no class type-args is
// inferrable here, which it otherwise would not be.

#[tokio::test]
async fn unbound_container_shapes_recovers_t_from_fields() {
    // §G/G1: ContainerShapes with EMPTY wire type-args (an unbound instance) met
    // by the forcing formal `ContainerShapes<T>` ⇒ T=int recovered from the field
    // VALUES, not from a wire type-arg.
    let shape = BEV::instance(
        "ContainerShapes",
        indexmap::IndexMap::from_iter([
            ("item", BEV::Int(1)),
            (
                "items",
                arr(
                    RuntimeTy::int(),
                    vec![BEV::Int(1), BEV::Int(2), BEV::Int(3)],
                ),
            ),
            ("by_key", int_map(RuntimeTy::int(), &[("k", BEV::Int(4))])),
            ("maybe", BEV::Null),
            ("mixed", BEV::Null),
        ]),
    );
    let out = call_infer(READ_T, "read_t", vec![shape]).await.unwrap();
    assert_eq!(out, BEV::String("int".into()));
}

#[tokio::test]
async fn unbound_recursive_recovers_t_from_fields() {
    // §G/G1: GenericRecursive with EMPTY wire type-args met by the forcing formal
    // `GenericRecursive<T>` ⇒ T=int recovered from `value`/`next` field values.
    let src = r#"
        class GenericRecursive<T> { value T next GenericRecursive<T>? }
        function list_head_t<T>(list: GenericRecursive<T>) -> string {
            type.of<T>().to_string()
        }
        function main() -> int { 0 }
    "#;
    let tail = BEV::instance(
        "GenericRecursive",
        indexmap::IndexMap::from_iter([("value", BEV::Int(8)), ("next", BEV::Null)]),
    );
    let head = BEV::instance(
        "GenericRecursive",
        indexmap::IndexMap::from_iter([("value", BEV::Int(7)), ("next", tail)]),
    );
    let out = call_infer(src, "list_head_t", vec![head]).await.unwrap();
    assert_eq!(out, BEV::String("int".into()));
}

#[tokio::test]
async fn unbound_outer_pair_with_bound_inner_recovers_all_vars() {
    // §G/G1 (nested, realistic): the caller forgot the OUTER subscript, so the
    // outer GenericPair is unbound (empty wire type-args), but its inner pairs are
    // bound (`GenericPair[int,str]`, `GenericPair[bool,float]`). The forcing formal
    // `GenericPair<GenericPair<A,B>, GenericPair<C,D>>` recurses into the outer's
    // field values and recovers A,B,C,D from the inner instances' wire type-args.
    let src = r#"
        class GenericPair<A, B> { first A second B }
        function extract<A, B, C, D>(a: GenericPair<GenericPair<A, B>, GenericPair<C, D>>) -> string {
            type.of<A | B | C | D>().to_string()
        }
        function main() -> int { 0 }
    "#;
    let inner1 = BEV::instance_generic(
        "GenericPair",
        vec![RuntimeTy::int(), RuntimeTy::string()],
        indexmap::IndexMap::from_iter([("first", BEV::Int(1)), ("second", s("a"))]),
    );
    let inner2 = BEV::instance_generic(
        "GenericPair",
        vec![RuntimeTy::bool(), RuntimeTy::float()],
        indexmap::IndexMap::from_iter([("first", BEV::Bool(true)), ("second", BEV::Float(1.5))]),
    );
    // Outer instance: EMPTY wire type-args (unbound).
    let arg = BEV::instance(
        "GenericPair",
        indexmap::IndexMap::from_iter([("first", inner1), ("second", inner2)]),
    );
    let out = call_infer(src, "extract", vec![arg]).await.unwrap();
    let BEV::String(r) = &out else {
        panic!("expected string, got {out:?}");
    };
    let r = r.as_str();
    assert!(
        r.contains("int") && r.contains("string") && r.contains("bool") && r.contains("float"),
        "expected all four vars recovered, got {r:?}"
    );
}

#[tokio::test]
async fn fully_unbound_nested_pair_recovers_all_vars_deeply() {
    // §G deep recovery: even when EVERY level is unbound (no wire type-args at the
    // outer OR inner instances), the forcing formal
    // `GenericPair<GenericPair<A,B>, GenericPair<C,D>>` drives recursion ALL the
    // way down — `reconstruct_unbound_instance_args` is formal-aware, so each
    // nested unbound instance is itself reconstructed against its slot's formal,
    // recovering A,B,C,D from the leaf field values. (Previously this stopped one
    // level down and the inner vars fell to `rust_type`.)
    let src = r#"
        class GenericPair<A, B> { first A second B }
        function extract<A, B, C, D>(a: GenericPair<GenericPair<A, B>, GenericPair<C, D>>) -> string {
            type.of<A | B | C | D>().to_string()
        }
        function main() -> int { 0 }
    "#;
    let inner1 = BEV::instance(
        "GenericPair",
        indexmap::IndexMap::from_iter([("first", BEV::Int(1)), ("second", s("a"))]),
    );
    let inner2 = BEV::instance(
        "GenericPair",
        indexmap::IndexMap::from_iter([("first", BEV::Bool(true)), ("second", BEV::Float(1.5))]),
    );
    let arg = BEV::instance(
        "GenericPair",
        indexmap::IndexMap::from_iter([("first", inner1), ("second", inner2)]),
    );
    let out = call_infer(src, "extract", vec![arg]).await.unwrap();
    let BEV::String(r) = &out else {
        panic!("expected string, got {out:?}");
    };
    let r = r.as_str();
    assert!(
        r.contains("int") && r.contains("string") && r.contains("bool") && r.contains("float"),
        "expected all four vars recovered via deep recursion, got {r:?}"
    );
}

// ── §D: covariant join over CONCRETE baml types (enum + class), not just leaves ─

#[tokio::test]
async fn triple_choose_join_includes_enum_and_concrete_class() {
    // §D over concrete actuals: the n-ary covariant join merges a primitive, an
    // ENUM, and a concrete BAML class ⇒ T = int | SomeEnum | StringIntPair. Unlike
    // the python layer (where a str-enum rides as `string`), a `Variant` actual is
    // unambiguously the enum type here.
    let src = r#"
        enum SomeEnum { VARIANT OTHER }
        class StringIntPair { my_string string my_int int }
        function triple_choose<T>(a: T, b: T, c: T) -> string {
            type.of<T>().to_string()
        }
        function main() -> int { 0 }
    "#;
    let enum_val = BEV::Variant {
        enum_name: "SomeEnum".to_string(),
        variant_name: "VARIANT".to_string(),
    };
    let class_val = BEV::instance(
        "StringIntPair",
        indexmap::IndexMap::from_iter([("my_string", s("a")), ("my_int", BEV::Int(1))]),
    );
    let out = call_infer(src, "triple_choose", vec![BEV::Int(5), enum_val, class_val])
        .await
        .unwrap();
    let BEV::String(r) = &out else {
        panic!("expected string, got {out:?}");
    };
    let r = r.as_str();
    assert!(
        r.contains("int") && r.contains("SomeEnum") && r.contains("StringIntPair"),
        "expected `int | SomeEnum | StringIntPair`, got {r:?}"
    );
}
