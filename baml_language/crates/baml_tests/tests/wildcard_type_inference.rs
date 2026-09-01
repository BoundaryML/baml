//! Regression tests for the `_` wildcard inference hole in type positions.
//!
//! Two related features land here (Linear B-230 and B-247):
//!
//!   * **B-230** — a `_` in a generic *type argument* of a `let` annotation is
//!     inferred from the initializer, keeping the rest of the annotation
//!     explicit. `let fs: baml.future.Future<int, _>[] = [spawn { … }]` adopts
//!     the spawn body's real error type instead of forcing the developer to
//!     spell it out (or trap themselves with `never`).
//!   * **B-247** — a `_` member in a `throws` clause (`throws AppError | _`) is
//!     an *open* contract: it absorbs the body's inferred (e.g. stdlib) throws
//!     without forcing an exhaustive re-declaration, while a plain `throws T`
//!     stays exhaustive.
//!
//! Both are filled during type checking from real inference — there is no
//! spawn/throws special-casing; `_` lowers to a single `Ty::Infer` hole.
//!
//! Tests here use `#[should_panic]` for compile-error assertions, which cannot be
//! expressed in the BAML corpus.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

const SLOW: &str = r#"
class MyErr { msg string }
function slow(x: int) -> int throws MyErr {
  if (x < 0) { throw MyErr { msg: "neg" }; }
  x
}
"#;

// ===========================================================================
// B-230 — `_` in a generic type argument
// ===========================================================================

/// `Future<int, _>` infers the spawned body's error type (`MyErr`) and runs.
#[tokio::test]
async fn wildcard_future_error_is_inferred_and_runs() {
    let src = format!(
        "{SLOW}\nfunction main() -> int {{\n  let fs: baml.future.Future<int, _>[] = [spawn {{ slow(1) }}];\n  await fs[0]\n}}\n"
    );
    let out = baml_test!(&src);
    assert!(
        matches!(out.result, Ok(BexExternalValue::Int(1))),
        "expected 1, got {:?}",
        out.result
    );
}

/// The inferred error type is *precise*, not erased to `unknown`: a `throws
/// never` enclosing function awaiting the future must still be rejected because
/// the body may throw `MyErr`.
#[tokio::test]
#[should_panic(expected = "[E0096]")]
async fn wildcard_future_error_is_not_erased() {
    let src = format!(
        "{SLOW}\nfunction main() -> int throws never {{\n  let fs: baml.future.Future<int, _>[] = [spawn {{ slow(1) }}];\n  await fs[0]\n}}\n"
    );
    let _ = baml_test!(&src);
}

/// `never` is NOT a "don't care": annotating a fallible spawn as
/// `Future<int, never>` is still a hard error (the regression the ticket calls
/// out — `_` is the ergonomic escape hatch, `never` keeps its meaning).
#[tokio::test]
#[should_panic(expected = "[E0001]")]
async fn wildcard_never_annotation_still_rejected() {
    let src = format!(
        "{SLOW}\nfunction main() -> int {{\n  let fs: baml.future.Future<int, never>[] = [spawn {{ slow(1) }}];\n  await fs[0]\n}}\n"
    );
    let _ = baml_test!(&src);
}

/// A `_` fills only its own slot — the explicit positions of the annotation are
/// still enforced. `Future<string, _>` against an `int`-returning body is a
/// type error, exactly as the filled `Future<string, MyErr>` would be.
#[tokio::test]
#[should_panic(expected = "[E0001]")]
async fn wildcard_does_not_mask_wrong_explicit_arg() {
    let src = format!(
        "{SLOW}\nfunction main() -> int {{\n  let fs: baml.future.Future<string, _>[] = [spawn {{ slow(1) }}];\n  0\n}}\n"
    );
    let _ = baml_test!(&src);
}

/// `_` is a general type-argument hole, not a spawn special-case: a `_` value
/// type in a `map<…>` annotation is inferred from a plain map initializer.
#[tokio::test]
async fn wildcard_map_value_is_inferred() {
    let out = baml_test!(
        r#"
        function main() -> int {
          let m: map<string, _> = { "a": 10, "b": 20 };
          m.get("b") ?? 0
        }
    "#
    );
    assert!(
        matches!(out.result, Ok(BexExternalValue::Int(20))),
        "expected 20, got {:?}",
        out.result
    );
}

/// `_` in a non-inferable position (a function return type) is a clean E0147
/// error, not a compiler panic — there is nothing to infer it from.
#[tokio::test]
#[should_panic(expected = "[E0147]")]
async fn wildcard_in_return_type_is_rejected() {
    let _ = baml_test!("function main() -> _ { 5 }");
}

/// Same for a parameter type.
#[tokio::test]
#[should_panic(expected = "[E0147]")]
async fn wildcard_in_param_type_is_rejected() {
    let _ = baml_test!("function f(x: _) -> int { 0 }\nfunction main() -> int { 0 }");
}

/// Same for a class field type.
#[tokio::test]
#[should_panic(expected = "[E0147]")]
async fn wildcard_in_field_type_is_rejected() {
    let _ = baml_test!("class C { x _ }\nfunction main() -> int { 0 }");
}

/// `_` is rejected in a generic bound (`<T extends _>`) — there is nothing to
/// infer a bound from.
#[tokio::test]
#[should_panic(expected = "[E0147]")]
async fn wildcard_in_generic_bound_is_rejected() {
    let _ = baml_test!("function f<T extends _>(x: T) -> int { 0 }\nfunction main() -> int { 0 }");
}

/// `_` is rejected in an interface `requires` clause.
#[tokio::test]
#[should_panic(expected = "[E0147]")]
async fn wildcard_in_requires_clause_is_rejected() {
    let _ = baml_test!("interface I requires _ {}\nfunction main() -> int { 0 }");
}

/// A `_` nested inside a thrown type (`throws Err<_>`) is rejected — only a
/// top-level `_` union member is the open-contract marker.
#[tokio::test]
#[should_panic(expected = "[E0147]")]
async fn wildcard_nested_in_throws_is_rejected() {
    let _ = baml_test!(
        "class Err<T> { v T }\nfunction f() -> int throws Err<_> { throw Err<int> { v: 1 }; }\nfunction main() -> int { 0 }"
    );
}

/// `hir_ty` recursively fills a uniquely alignable hole inside a union member;
/// invariant class matching does not erase the inferred member.
#[tokio::test]
async fn wildcard_nested_union_member_is_inferred() {
    let out = baml_test!(
        "class Box<T> { v T }\nfunction main() -> int | string { let b: Box<int | _> = Box<int|string> { v: 1 }; b.v }"
    );
    assert!(
        matches!(
            &out.result,
            Ok(BexExternalValue::Union { value, .. })
                if matches!(value.as_ref(), BexExternalValue::Int(1))
        ),
        "expected 1, got {:?}",
        out.result
    );
}

/// An interface required method has no body to infer an open `throws … | _`
/// from, and its declared throws is compared structurally during conformance
/// checking — so even a top-level `_` is rejected here (cleanly, not a panic).
#[tokio::test]
#[should_panic(expected = "[E0147]")]
async fn wildcard_in_interface_method_throws_is_rejected() {
    let _ = baml_test!(
        "interface I {\n  function run(self) -> int throws _\n}\nfunction main() -> int { 0 }"
    );
}

// ===========================================================================
// B-247 — `_` in a `throws` clause + stdlib throw precision
// ===========================================================================

/// `throws BadInput | _` compiles even though the body transitively throws the
/// stdlib `baml.json.*` errors: the `_` absorbs them.
#[tokio::test]
async fn throws_wildcard_absorbs_stdlib_throws() {
    let out = baml_test!(
        r#"
        class BadInput { msg string }
        function parse(s: string) -> int throws BadInput | _ {
          let v: json = baml.json.from_string(s);
          throw BadInput { msg: "nope" };
        }
        function main() -> int { 0 }
    "#
    );
    assert!(
        matches!(out.result, Ok(BexExternalValue::Int(0))),
        "expected 0, got {:?}",
        out.result
    );
}

/// The open contract is still SOUND: a caller declaring `throws never` sees the
/// full inferred union (the named `BadInput` plus the stdlib json throws), not
/// just the declared member.
#[tokio::test]
#[should_panic(expected = "ParseError")]
async fn throws_wildcard_caller_sees_full_union() {
    let _ = baml_test!(
        r#"
        class BadInput { msg string }
        function parse(s: string) -> int throws BadInput | _ {
          let v: json = baml.json.from_string(s);
          throw BadInput { msg: "nope" };
        }
        function caller(s: string) -> int throws never {
          parse(s)
        }
        function main() -> int { 0 }
    "#
    );
}

/// A plain `throws BadInput` (no `_`) stays exhaustive: an undeclared stdlib
/// throw is still an E0096 violation. The `_` is opt-in, not the default.
#[tokio::test]
#[should_panic(expected = "[E0096]")]
async fn throws_plain_stays_exhaustive() {
    let _ = baml_test!(
        r#"
        class BadInput { msg string }
        function parse(s: string) -> int throws BadInput {
          let v: json = baml.json.from_string(s);
          throw BadInput { msg: "nope" };
        }
        function main() -> int { 0 }
    "#
    );
}
