//! Phase 6 tests: Generic type variable binding and builtin method resolution.
//!
//! Verifies that `Ty::List`, `Ty::Map`, and `Ty::String` correctly
//! resolve methods to the builtin `.baml` stub declarations with type variable
//! substitution applied.

use super::support::{expr_type_in_function, make_db, render_tir};
use crate::engine::TestDbExt;

// ── Array method resolution ───────────────────────────────────────────────────

#[test]
fn array_length_returns_int() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f(arr: int[]) -> int { return arr.length(); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(arr: int[]) -> int throws never {
      { : never
        return arr.length() : int
      }
    }
    ");
}

#[test]
fn array_at_returns_element_type_int() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f(arr: int[]) -> int? { return arr.at(0); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(arr: int[]) -> int | null throws never {
      { : never
        return arr.at(0) : int | null
      }
    }
    ");
}

#[test]
fn array_at_returns_element_type_string() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f(arr: string[]) -> string? { return arr.at(0); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(arr: string[]) -> string | null throws never {
      { : never
        return arr.at(0) : string | null
      }
    }
    ");
}

#[test]
fn array_join_returns_string() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f(arr: string[]) -> string { return arr.join(","); }"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f(arr: string[]) -> string throws never {
      { : never
        return arr.join(",") : string
      }
    }
    "#);
}

#[test]
fn user_defined_array_does_not_bridge_like_builtin_array() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class Array<T> {}

function keep<T>(value: Array<T>, fallback: T) -> T {
    fallback
}

function f(xs: int[]) -> int {
    return keep(xs, 0)
}
"#,
    );

    let tir = render_tir(&db, file);
    assert!(
        tir.contains("type mismatch: expected Array<int>, got int[]"),
        "expected nominal user.Array<T> to stay distinct from builtin int[], got:\n{tir}"
    );
}

// ── Map method resolution ─────────────────────────────────────────────────────

#[test]
fn map_keys_returns_key_type_array() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f(m: map<string, int>) -> string[] { return m.keys(); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(m: map<string, int>) -> string[] throws never {
      { : never
        return m.keys() : string[]
      }
    }
    ");
}

#[test]
fn map_values_returns_value_type_array() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f(m: map<string, int>) -> int[] { return m.values(); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(m: map<string, int>) -> int[] throws never {
      { : never
        return m.values() : int[]
      }
    }
    ");
}

#[test]
fn map_has_returns_bool() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f(m: map<string, int>) -> bool { return m.has("x"); }"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f(m: map<string, int>) -> bool throws never {
      { : never
        return m.has("x") : bool
      }
    }
    "#);
}

#[test]
fn map_length_returns_int() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f(m: map<string, int>) -> int { return m.length(); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(m: map<string, int>) -> int throws never {
      { : never
        return m.length() : int
      }
    }
    ");
}

// ── String method resolution ──────────────────────────────────────────────────

#[test]
fn string_length_returns_int() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f(s: string) -> int { return s.length(); }"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(s: string) -> int throws never {
      { : never
        return s.length() : int
      }
    }
    ");
}

#[test]
fn string_split_returns_string_array() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f(s: string) -> string[] { return s.split(","); }"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f(s: string) -> string[] throws never {
      { : never
        return s.split(",") : string[]
      }
    }
    "#);
}

#[test]
fn string_includes_returns_bool() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f(s: string) -> bool { return s.includes("ell"); }"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f(s: string) -> bool throws never {
      { : never
        return s.includes("ell") : bool
      }
    }
    "#);
}

#[test]
fn string_to_lower_case_returns_string() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f(s: string) -> string { return s.to_lower_case(); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(s: string) -> string throws never {
      { : never
        return s.to_lower_case() : string
      }
    }
    ");
}

// ── Let binding with inferred type from builtin methods ───────────────────────

#[test]
fn let_inferred_from_array_length() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f(arr: int[]) -> int { let len = arr.length(); return len; }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(arr: int[]) -> int throws never {
      { : never
        let len = arr.length() : int
        return len : int
      }
    }
    ");
}

#[test]
fn let_inferred_from_array_at() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f(arr: int[]) -> int? { let x = arr.at(0); return x; }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(arr: int[]) -> int | null throws never {
      { : never
        let x = arr.at(0) : int | null
        return x : int | null
      }
    }
    ");
}

#[test]
fn let_inferred_from_map_keys() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f(m: map<string, int>) -> string[] { let k = m.keys(); return k; }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(m: map<string, int>) -> string[] throws never {
      { : never
        let k = m.keys() : string[]
        return k : string[]
      }
    }
    ");
}

// ── Media type method resolution ──────────────────────────────────────────────

#[test]
fn image_url_returns_optional_string() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f(img: image) -> string? { return img.url(); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(img: image) -> string | null throws never {
      { : never
        return img.url() : string | null
      }
    }
    ");
}

#[test]
fn image_base64_returns_string() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f(img: image) -> string { return img.base64(); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(img: image) -> string throws never {
      { : never
        return img.base64() : string
      }
    }
    ");
}

#[test]
fn image_mime_type_returns_optional_string() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f(img: image) -> string? { return img.mime_type(); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(img: image) -> string | null throws never {
      { : never
        return img.mime_type() : string | null
      }
    }
    ");
}

#[test]
fn pdf_url_returns_optional_string() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f(doc: pdf) -> string? { return doc.url(); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(doc: pdf) -> string | null throws never {
      { : never
        return doc.url() : string | null
      }
    }
    ");
}

#[test]
fn audio_base64_returns_string() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f(a: audio) -> string { return a.base64(); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(a: audio) -> string throws never {
      { : never
        return a.base64() : string
      }
    }
    ");
}

#[test]
fn video_file_returns_optional_string() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f(v: video) -> string? { return v.file(); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(v: video) -> string | null throws never {
      { : never
        return v.file() : string | null
      }
    }
    ");
}

#[test]
fn image_missing_method_produces_unresolved_member() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f(img: image) -> int { return img.nonexistent(); }",
    );
    let output = render_tir(&db, file);
    assert!(
        output.contains("has no member"),
        "Expected 'has no member' in output, got:\n{output}"
    );
}

// ── Static constructors via primitive type name ──────────────────────────────

#[test]
fn image_static_from_url() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f() -> image { return image.from_url("example.com/img.png", null); }"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f() -> image throws never {
      { : never
        return image.from_url("example.com/img.png", null) : image
      }
    }
    "#);
}

#[test]
fn pdf_static_from_base64() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f() -> pdf { return pdf.from_base64("base64data", null); }"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f() -> pdf throws never {
      { : never
        return pdf.from_base64("base64data", null) : pdf
      }
    }
    "#);
}

#[test]
fn audio_static_from_file() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f() -> audio { return audio.from_file("song.mp3", null); }"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f() -> audio throws never {
      { : never
        return audio.from_file("song.mp3", null) : audio
      }
    }
    "#);
}

#[test]
fn video_static_from_url() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f() -> video { return video.from_url("example.com/v.mp4", null); }"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f() -> video throws never {
      { : never
        return video.from_url("example.com/v.mp4", null) : video
      }
    }
    "#);
}

// ── Error: non-existent method on builtin type ─────────────────────────────────

#[test]
fn array_missing_method_produces_unresolved_member() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f(arr: int[]) -> int { return arr.nonexistent(); }",
    );
    let output = render_tir(&db, file);
    // Should produce an UnresolvedMember diagnostic
    assert!(
        output.contains("has no member"),
        "Expected 'has no member' in output, got:\n{output}"
    );
}

#[test]
fn map_missing_method_produces_unresolved_member() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f(m: map<string, int>) -> int { return m.bogus(); }",
    );
    let output = render_tir(&db, file);
    assert!(
        output.contains("has no member"),
        "Expected 'has no member' in output, got:\n{output}"
    );
}

#[test]
fn string_missing_method_produces_unresolved_member() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        "function f(s: string) -> int { return s.doesNotExist(); }",
    );
    let output = render_tir(&db, file);
    assert!(
        output.contains("has no member"),
        "Expected 'has no member' in output, got:\n{output}"
    );
}

// ── Snapshot: full rendering of a function using builtin methods ──────────────

#[test]
fn snapshot_builtin_method_calls() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f(arr: string[], m: map<string, int>, s: string) -> int {
  let len = arr.length();
  let keys = m.keys();
  let parts = s.split(",");
  return len;
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

// ── Optional call (?.()) type inference ──────────────────────────────────────

#[test]
fn optional_call_basic() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(callback: ((x: int) -> int throws never)?) -> int? {
    return callback?.(42)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(callback: ((x: int) -> int throws never) | null) -> int | null throws never {
      { : never
        return callback?.(42) : int | null
      }
    }
    ");
}

#[test]
fn optional_call_generic_map() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(arr: int[]?) -> int[]? {
    return arr?.map?.((x: int) -> int { x + 1 })
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(arr: int[] | null) -> int[] | null throws never {
      { : never
        return arr?.map?.((x: int) -> int { ... }) : int[] | null
      }
    }
    lambda user.f {
    }
    ");
}

#[test]
fn direct_optional_method_call_generic_map() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(arr: int[]?) -> int[]? {
    return arr?.map((x) -> { x + 1 })
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(arr: int[] | null) -> int[] | null throws never {
      { : never
        return arr?.map((x) -> { ... }) : int[] | null
      }
    }
    lambda user.f {
    }
    ");
}

#[test]
fn optional_call_arg_type_checking() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(callback: ((x: int) -> int throws never)?) -> int? {
    return callback?.("wrong")
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f(callback: ((x: int) -> int throws never) | null) -> int | null throws never {
      { : never
        return callback?.("wrong") : int | null
      }
      !! 87..94: type mismatch: expected int, got "wrong"
    }
    "#);
}

#[test]
fn optional_call_checks_higher_order_function_arguments() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function demo(cb: (((x: int) -> int throws never) -> int throws never)?) -> int? throws never {
  let risky = (x: int) -> int throws string {
    throw "boom"
  }

  cb?.(risky)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.demo(cb: (((x: int) -> int throws never) -> int throws never) | null) -> int | null throws never {
      { : int | null
        let risky = : (x: int) -> int throws string
          (x: int) -> int throws string { ... } : (x: int) -> int throws string
            {
              throw "boom"
            }
        cb?.(risky) : int | null
      }
      !! 172..177: type mismatch: expected (x: int) -> int throws never, got (x: int) -> int throws string
    }
    lambda user.demo {
    }
    "#);
}

#[test]
fn optional_call_through_type_alias() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
type MaybeFn = ((x: int) -> int throws never)?
function f(callback: MaybeFn) -> int? {
    return callback?.(42)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    type user.MaybeFn = ((x: int) -> int throws never) | null
    function user.f(callback: user.MaybeFn) -> int | null throws never {
      { : never
        return callback?.(42) : int | null
      }
      !! 99..113: did you mean `callback(42)`? `callback?.(42)` is unnecessary, because `callback` cannot be null
    }
    type user.MaybeFn$stream = unknown | null
    ");
}

#[test]
fn optional_field_access_through_type_alias() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class User { name string }
type MaybeUser = User?
function f(u: MaybeUser) -> string? {
    return u?.name
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    class user.User {
      name: string
    }
    type user.MaybeUser = user.User | null
    function user.f(u: user.MaybeUser) -> string | null throws never {
      { : never
        return u?.name : string | null
      }
      !! 100..107: did you mean `u.name`? `u?.name` is unnecessary, because `u` cannot be null
    }
    class user.User$stream {
      name: string | null
    }
    type user.MaybeUser$stream = user.User$stream | null
    ");
}

#[test]
fn optional_index_through_type_alias() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
type MaybeInts = int[]?
function f(xs: MaybeInts) -> int? {
    return xs?.[0]
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    type user.MaybeInts = int[] | null
    function user.f(xs: user.MaybeInts) -> int | null throws never {
      { : never
        return xs?.[0] : int | null
      }
    }
    type user.MaybeInts$stream = int[] | null
    ");
}

#[test]
fn optional_call_expected_nonoptional_still_mismatches() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(cb: ((x: int) -> int throws never)?) -> int {
    return cb?.(1)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(cb: ((x: int) -> int throws never) | null) -> int throws never {
      { : never
        return cb?.(1) : int | null
      }
      !! 69..76: type mismatch: expected int, got int | null
    }
    ");
}

#[test]
fn optional_call_nullable_return_preserves_phase0() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(cb: ((x: int) -> string? throws never)?) -> string? {
    return cb?.(1)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(cb: ((x: int) -> string | null throws never) | null) -> string | null throws never {
      { : never
        return cb?.(1) : string | null
      }
    }
    ");
}

#[test]
fn optional_call_null_short_circuit_still_checks_args() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f() -> null {
    let cb = null
    return cb?.(unknown_name)
}
"#,
    );
    let output = render_tir(&db, file);
    assert!(
        output.contains("unresolved name: unknown_name"),
        "Expected unresolved-name diagnostic from argument inference, got:\n{output}"
    );
}

#[test]
fn optional_call_null_short_circuit_does_not_emit_call_diagnostics() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f() -> null {
    let cb = null
    return cb?.(1, 2)
}
"#,
    );
    let output = render_tir(&db, file);
    assert!(
        !output.contains("argument(s)"),
        "Did not expect call-site arity diagnostics for statically-null optional call, got:\n{output}"
    );
}

#[test]
fn optional_call_null_short_circuit_respects_expected() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f() -> int {
    let cb = null
    return cb?.(1)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f() -> int throws never {
      { : never
        let cb = null : null
        return cb?.(1) : null | !error
      }
      !! 52..54: `never` is not a function — it cannot be called
    }
    ");
}

#[test]
fn optional_push_establishes_element_type() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f() -> null {
    let xs = []
    xs?.push?.(1)
    xs.push("a")
    return null
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f() -> null throws never {
      { : never
        let xs = [] : int[]
        xs?.push?.(1) : int
        xs.push("a") : int
        return null : null
      }
      !! 70..73: type mismatch: expected int, got "a"
    }
    "#);
}

#[test]
fn empty_array_reassignment_keeps_declared_element_type() {
    // Regression: `x = []` must not drop `x`'s declared `int[]`. The assigned
    // empty would otherwise become an adoptable evolving-never local, and a
    // later `push` would establish a wrong element type under the declared one
    // (unsound). `push("hello")` must be rejected against the retained `int[]`.
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f() -> null {
    let x: int[] = [1]
    x = []
    x.push("hello")
    return null
}
"#,
    );
    let tir = render_tir(&db, file);
    // hir_ty keeps literal grain in mismatch payloads (ruled render family).
    assert!(
        tir.contains("type mismatch: expected int, got \"hello\""),
        "expected `x.push(\"hello\")` to be rejected after `x = []`; got:\n{tir}"
    );
}

#[test]
fn generic_construction_cannot_infer_param_from_empty_field() {
    // Regression (F6): constructing a generic class whose parameter no field
    // determines — here `T` from `Box { items: [] }` — must report `cannot infer
    // type parameter`, not silently produce an unspecialized `Box`. The
    // unspecialized form would otherwise reach MIR lowering carrying a bare type
    // variable and trip `tir2_to_template`'s `unreachable!`; the diagnostic keeps
    // the program out of lowering (which only runs error-free).
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class Box<T> {
    items: T[]
}
function f() -> int {
    let b = Box { items: [] }
    0
}
"#,
    );
    let tir = render_tir(&db, file);
    assert!(
        tir.contains("cannot infer type parameter `T`"),
        "expected `Box {{ items: [] }}` to report an uninferrable `T`; got:\n{tir}"
    );
}

#[test]
fn container_param_default_element_error_reported_once() {
    // Regression (H1): a container-literal param default with a mismatched
    // element must report the element error exactly once. The default was
    // previously typed twice (infer then check), duplicating the diagnostic.
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class Money { cents: bigint }
function pay(items: Money[] = [Money { cents: 1 }]) -> int { 0 }
"#,
    );
    let tir = render_tir(&db, file);
    let count = tir.matches("expected bigint, got 1").count();
    assert_eq!(
        count, 1,
        "param-default element mismatch must be reported once; got {count}:\n{tir}"
    );
}

#[test]
fn generic_construction_does_not_report_phantom_param() {
    // Regression (M3): a class type parameter used by no field is a phantom that
    // construction cannot determine — it must NOT be reported as
    // `CannotInferTypeParameter` (only a field-constrained param is).
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class Pair<T, U> {
    first: T[]
}
function f() -> int {
    let p = Pair { first: [1] }
    0
}
"#,
    );
    let tir = render_tir(&db, file);
    assert!(
        !tir.contains("cannot infer type parameter `U`"),
        "phantom param `U` (used by no field) must not be reported; got:\n{tir}"
    );
    assert!(
        !tir.contains("cannot infer type parameter `T`"),
        "field-determined `T` must infer from `first: [1]`; got:\n{tir}"
    );
}

#[test]
fn assignment_nested_empty_container_adopts_declared_type() {
    // Regression (M2): reassigning a nested empty literal to a declared
    // nested-container local adopts the declared element types *recursively* —
    // the inner `[]` must not leak `EvolvingList(Never)` under `int[][]`.
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f() -> null {
    let x: int[][] = [[1]]
    x = [[]]
    return null
}
"#,
    );
    let tir = render_tir(&db, file);
    // The outer renders `int[][]` only if the inner `[]` adopted `int[]`;
    // without recursive adoption it would render the evolving-never `_[][]`.
    assert!(
        tir.contains("x = [[]] : int[][]"),
        "`x = [[]]` should adopt the declared `int[][]` recursively; got:\n{tir}"
    );
}

#[test]
fn catch_handler_empty_array_adopts_expected_type() {
    // Regression (M1): in a checking position a catch handler body adopts the
    // expected type — an empty `[]` handler becomes the declared element type,
    // not `unknown`/`EvolvingList(Never)`.
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
enum Err { Boom }
function risky(x: int) -> int[] throws Err {
    if x == 0 { throw Err.Boom }
    return [1]
}
function f() -> int[] {
    return risky(0) catch (e) {
        Err.Boom => []
    }
}
"#,
    );
    let tir = render_tir(&db, file);
    assert!(
        tir.contains("[] : int[]"),
        "catch handler `[]` should adopt `int[]`; got:\n{tir}"
    );
}

#[test]
fn direct_optional_push_establishes_element_type() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f() -> null {
    let xs = []
    xs?.push(1)
    xs.push("a")
    return null
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f() -> null throws never {
      { : never
        let xs = [] : int[]
        xs?.push(1) : int
        xs.push("a") : int
        return null : null
      }
      !! 68..71: type mismatch: expected int, got "a"
    }
    "#);
}

#[test]
fn optional_push_returns_optional_int() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(xs: int[]?) -> int? {
    return xs?.push?.(2)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(xs: int[] | null) -> int | null throws never {
      { : never
        return xs?.push?.(2) : int | null
      }
    }
    ");
}

#[test]
fn push_establishment_updates_let_binding_type_for_function_values() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function cb() -> int {
    return 1
}

function f() -> null {
    let callbacks = []
    callbacks.push(cb)
    return null
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.cb() -> int throws never {
      { : never
        return 1 : 1
      }
    }
    function user.f() -> null throws never {
      { : never
        let callbacks = [] : (() -> int throws never)[]
        callbacks.push(cb) : int
        return null : null
      }
    }
    ");
}

#[test]
fn optional_push_inner_callee_stays_optional_callable() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(xs: int[]?) -> int? {
    return xs?.push?.(2)
}
"#,
    );
    assert_eq!(
        expr_type_in_function(&db, file, "f", "xs?.push"),
        // hir_ty peels the chain null and binds the receiver on the callee
        // value (the ruled chain-callee-peel family).
        "(item: int) -> int throws never"
    );
}

#[test]
fn named_wrapper_value_preserves_effect_param() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function wrap(cb: (x: int) -> string) -> string {
    return cb(1)
}

function f() -> null {
    let g = wrap
    g
    return null
}
"#,
    );
    assert_eq!(
        expr_type_in_function(&db, file, "f", "g"),
        // hir_ty instantiates value-refs at the reference (the S16 ruled
        // "tir-uninstantiated" render family): the unused effect param
        // solves to `never`.
        "(cb: (x: int) -> string throws never) -> string throws never"
    );
}

#[test]
fn named_wrapper_value_that_catches_callback_has_never_callable_throws() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function wrap(cb: (x: int) -> string) -> string {
    cb(1) catch (e) { _ => "fallback" }
}

function f() -> null {
    let g = wrap
    g
    return null
}
"#,
    );
    assert_eq!(
        expr_type_in_function(&db, file, "f", "g"),
        // hir_ty instantiates the effect param at the value ref (ruled family).
        "(cb: (x: int) -> string throws never) -> string throws never"
    );
}

#[test]
fn returned_wrapper_value_preserves_explicit_callback_throws() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function forward(cb: (x: int) -> int throws string) -> int {
    return cb(1)
}

function make() -> ((cb: (x: int) -> int throws string) -> int throws string) {
    return forward
}

function f() -> null {
    let g = make()
    g
    return null
}
"#,
    );
    assert_eq!(
        expr_type_in_function(&db, file, "f", "g"),
        "(cb: (x: int) -> int throws string) -> int throws string"
    );
}

#[test]
fn catch_wrapped_wrapper_value_preserves_callable_throws() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function wrap(cb: () -> int throws string) -> int {
    return cb() catch (e) {
        _ => wrap(cb)
    }
}

function f() -> null {
    let g = wrap
    g
    return null
}
"#,
    );
    assert_eq!(
        expr_type_in_function(&db, file, "f", "g"),
        // The catch-all discharges every fact, so the instantiated surface
        // is `throws never` (hir_ty; more precise than TIR's symbolic form).
        "(cb: () -> int throws string) -> int throws never"
    );
}

#[test]
fn bound_method_value_preserves_declared_throws() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class Worker {
    factor int

    function risky(self, value: int) -> int throws string {
        if (value < 0) { throw "negative" }
        self.factor * value
    }
}

function f(worker: Worker) -> null {
    let cb = worker.risky
    cb
    return null
}
"#,
    );
    assert_eq!(
        expr_type_in_function(&db, file, "f", "cb"),
        "(value: int) -> int throws string"
    );
}

#[test]
fn builtin_member_value_preserves_declared_throws() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(file: baml.fs.File) -> null {
    let read = file.read
    read
    return null
}
"#,
    );
    let ty = expr_type_in_function(&db, file, "f", "read");
    assert!(
        ty.starts_with("(limit: int) -> uint8array | null throws "),
        "expected builtin member value to preserve declared throws, got `{ty}`"
    );
    assert!(
        ty.contains("Io"),
        "expected builtin member value throws surface to reference Io, got `{ty}`"
    );
}

#[test]
fn builtin_map_method_value_preserves_callback_surface() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(xs: int[]) -> null {
    let m = xs.map
    m
    return null
}
"#,
    );
    assert_eq!(
        expr_type_in_function(&db, file, "f", "m"),
        // A standalone generic-method value has no call to solve `U`/`E`;
        // hir_ty marks the unresolved slots with the error sentinel
        // (ruled "tir-uninstantiated" family kept them symbolic).
        "(f: (int) -> !error throws !error) -> !error[] throws !error"
    );
}

#[test]
fn wrapper_value_around_builtin_map_preserves_callback_surface() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function builtin_map(cb: (value: int) -> int, values: int[]) -> int[] {
    return values.map(cb)
}

function f() -> null {
    let g = builtin_map
    g
    return null
}
"#,
    );
    assert_eq!(
        expr_type_in_function(&db, file, "f", "g"),
        // Effect param instantiated at the value ref (ruled family).
        "(cb: (value: int) -> int throws never, values: int[]) -> int[] throws never"
    );
}

#[test]
fn lambda_value_preserves_explicit_throws() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f() -> null {
    let risky = (x: int) -> int throws string {
        if (x < 0) { throw "negative" }
        x
    }
    risky
    return null
}
"#,
    );
    assert_eq!(
        expr_type_in_function(&db, file, "f", "risky"),
        "(x: int) -> int throws string"
    );
}

#[test]
fn stored_lambda_with_omitted_throws_infers_throws_in_expr_type() {
    // An UNANNOTATED lambda infers its throws surface from its body — a
    // lambda throws what it throws (BEP-034 middleware relies on this for
    // body wraps like `() -> { original() }` where the throws is the
    // enclosing fn's generic `E`). The stored value's type carries the
    // inferred, concrete surface instead of a blanket `never`.
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f() -> null {
    let risky = (x: int) -> int {
        throw "boom"
    }
    risky
    return null
}
"#,
    );
    // FLIPPED: an INFERRED throws surface keeps literal grain (the
    // spec's callback_effect_param_flows_through fixture pins it; TIR
    // diverged by widening thrown literals at the surface).
    assert_eq!(
        expr_type_in_function(&db, file, "f", "risky"),
        "(x: int) -> int throws \"boom\""
    );
}

#[test]
fn returned_triple_nested_lambda_reads_cleanly() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f() -> null {
    let triple = () -> {
        let middle = () -> {
            let inner = (n: int) -> int { n }
            inner
        }
        middle
    }
    triple
    return null
}
"#,
    );
    assert_eq!(
        expr_type_in_function(&db, file, "f", "triple"),
        "() -> (() -> ((n: int) -> int throws never) throws never) throws never"
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f() -> null throws never {
      { : never
        let triple = : () -> (() -> ((n: int) -> int throws never) throws never) throws never
          () -> { ... } : () -> (() -> ((n: int) -> int throws never) throws never) throws never
            {
              let middle = ...
                () -> { ... }
                  {
                    let inner = ...
                      (n: int) -> int { ... }
                        {
                          n
                        }
                    inner
                  }
              middle
            }
        triple : () -> (() -> ((n: int) -> int throws never) throws never) throws never
        return null : null
      }
    }
    lambda user.f {
    }
    lambda user.f {
    }
    lambda user.f {
    }
    "#);
}

#[test]
fn returned_quadruple_nested_lambda_reads_cleanly() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f() -> null {
    let quadruple = () -> {
        let third = () -> {
            let second = () -> {
                let first = (n: int) -> int { n }
                first
            }
            second
        }
        third
    }
    quadruple
    return null
}
"#,
    );
    assert_eq!(
        expr_type_in_function(&db, file, "f", "quadruple"),
        "() -> (() -> (() -> ((n: int) -> int throws never) throws never) throws never) throws never"
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f() -> null throws never {
      { : never
        let quadruple = : () -> (() -> (() -> ((n: int) -> int throws never) throws never) throws never) throws never
          () -> { ... } : () -> (() -> (() -> ((n: int) -> int throws never) throws never) throws never) throws never
            {
              let third = ...
                () -> { ... }
                  {
                    let second = ...
                      () -> { ... }
                        {
                          let first = ...
                            (n: int) -> int { ... }
                              {
                                n
                              }
                          first
                        }
                    second
                  }
              third
            }
        quadruple : () -> (() -> (() -> ((n: int) -> int throws never) throws never) throws never) throws never
        return null : null
      }
    }
    lambda user.f {
    }
    lambda user.f {
    }
    lambda user.f {
    }
    lambda user.f {
    }
    "#);
}

#[test]
fn plain_push_fast_path_still_checked_against_expected() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(xs: int[]) -> string {
    return xs.push(1)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(xs: int[]) -> string throws never {
      { : never
        return xs.push(1) : int
      }
      !! 46..56: type mismatch: expected string, got int
    }
    ");
}

#[test]
fn optional_push_fast_path_still_checked_against_expected() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(xs: int[]?) -> string {
    return xs?.push?.(1)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(xs: int[] | null) -> string throws never {
      { : never
        return xs?.push?.(1) : int | null
      }
      !! 47..60: type mismatch: expected string, got int | null
    }
    ");
}

#[test]
fn optional_call_lambda_contextual_typing() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(arr: int[]?) -> int[]? {
    return arr?.map?.((x) -> { x + 1 })
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(arr: int[] | null) -> int[] | null throws never {
      { : never
        return arr?.map?.((x) -> { ... }) : int[] | null
      }
    }
    lambda user.f {
    }
    ");
}

#[test]
fn optional_call_lambda_with_explicit_types() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(arr: int[]?) -> int[]? {
    return arr?.map?.((x: int) -> int { x + 1 })
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(arr: int[] | null) -> int[] | null throws never {
      { : never
        return arr?.map?.((x: int) -> int { ... }) : int[] | null
      }
    }
    lambda user.f {
    }
    ");
}

#[test]
fn optional_call_builtin_string_method() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(s: string?) -> string[]? {
    return s?.split?.(",")
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f(s: string | null) -> string[] | null throws never {
      { : never
        return s?.split?.(",") : string[] | null
      }
    }
    "#);
}

#[test]
fn optional_call_builtin_map_method() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(m: map<string, int>?) -> string[]? {
    return m?.keys?.()
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(m: map<string, int> | null) -> string[] | null throws never {
      { : never
        return m?.keys?.() : string[] | null
      }
    }
    ");
}

#[test]
fn optional_call_unnecessary_chaining_diagnostic() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(callback: (x: int) -> int) -> int? {
    return callback?.(42)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(callback: (x: int) -> int throws __effect_param_0) -> int | null throws never {
      { : never
        return callback?.(42) : int
      }
      !! 60..74: did you mean `callback(42)`? `callback?.(42)` is unnecessary, because `callback` cannot be null
    }
    ");
}

#[test]
fn parenthesized_optional_method_call_breaks_chain() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class User {
    function getName(self) -> string { self.name }
    name string
}

function f(u: User?) -> string {
    return (u?.getName)()
}
"#,
    );
    let output = render_tir(&db, file);
    // hir_ty types the failed call with the error sentinel
    // (replace-with-error; TIR used `unknown`).
    assert!(
        output.contains("return u?.getName() : !error"),
        "Expected broken-chain call result in output, got:\n{output}"
    );
    assert!(
        output.contains("cannot be called"),
        "Expected broken-chain call diagnostic, got:\n{output}"
    );
}

#[test]
fn optional_call_arity_mismatch() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(callback: ((x: int) -> int throws never)?) -> int? {
    return callback?.(1, 2)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(callback: ((x: int) -> int throws never) | null) -> int | null throws never {
      { : never
        return callback?.(1, 2) : int | null
      }
      !! 76..92: expected 1 argument(s), got 2
    }
    ");
}

#[test]
fn index_assignment_establishment_updates_let_binding_type() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f() -> int {
    let xs = []
    xs[0] = 1
    return xs[0]
}
"#,
    );
    let output = render_tir(&db, file);

    // hir_ty types the evolving empty through inference vars - the binding
    // solves straight to `int[]` (no separate evolving display).
    assert!(
        output.contains("let xs = [] : int[]"),
        "expected indexed assignment to sync the let binding type, got:\n{output}"
    );
    assert!(
        !output.contains("type mismatch"),
        "did not expect indexed assignment establishment to produce a mismatch, got:\n{output}"
    );
}

#[test]
fn lambda_body_container_establishment_does_not_leak_to_parent_scope() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f() -> int {
    let xs = []
    let _f = () -> int {
        let xs = []
        xs.push("inner")
        0
    }
    xs.push(1)
    return xs[0]
}
"#,
    );
    let output = render_tir(&db, file);

    // hir_ty solves the parent's evolving empty to `int[]` directly.
    assert!(
        output.contains("let xs = [] : int[]"),
        "expected parent xs binding to be established by parent push, got:\n{output}"
    );
    assert!(
        !output.contains("type mismatch"),
        "lambda-local container establishment should not affect parent xs, got:\n{output}"
    );
}

#[test]
fn for_body_container_assignment_establishes_outer_type() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f() -> int {
    let xs = []
    for (let n in []) {
        xs.push("not guaranteed")
    }
    xs.push(1)
    return xs[0]
}
"#,
    );
    let output = render_tir(&db, file);

    // hir_ty solves the element from the FIRST establishment (string) and
    // re-judges the later push at finalize with literal grain.
    assert!(
        output.contains("let xs = [] : string[]"),
        "expected xs to be established by the first push in the loop body, got:\n{output}"
    );
    assert!(
        output.contains("type mismatch: expected string, got 1"),
        "post-loop push should be checked against the loop-established element type, got:\n{output}"
    );
}

#[test]
fn refutable_array_pattern_is_rejected_in_for_binding() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(rows: int[][]) -> int {
    let total = 0
    for (let [let x] in rows) {
        total += x
    }
    return total
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        output.contains("refutable pattern in for-let binding"),
        "exact array pattern in for-let should be rejected as refutable, got:\n{output}"
    );
}

#[test]
fn or_pattern_same_binding_with_conflicting_types_is_rejected() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class A {
    field int
}

class B {
    field string
}

function f(value: A | B) -> int {
    match (value) {
        A { field: let x } | B { field: let x } => 1
    }
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        output.contains("Or-pattern alternatives bind `x` with conflicting types"),
        "same or-pattern binding name with incompatible types should be rejected, got:\n{output}"
    );
}

#[test]
fn class_destructure_unknown_field_in_let_reports_field_error() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class Point {
    value int
}

function f() -> int {
    let Point { valeu } = Point { value: 1 }
    valeu
    return 0
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        output.contains("class `Point` has no field `valeu`"),
        "unknown field in let-pattern should be diagnosed at the pattern, got:\n{output}"
    );
    assert!(
        output.contains("Did you mean `value`?"),
        "unknown let-pattern field should include typo suggestion, got:\n{output}"
    );
    assert!(
        !output.contains("unresolved name: valeu"),
        "unknown field pattern should not cascade into unresolved binding errors, got:\n{output}"
    );
}

#[test]
fn class_destructure_unknown_field_in_match_does_not_make_next_arm_unreachable() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class Point {
    x int
    y int
}

function f(p: Point) -> string {
    match (p) {
        Point { xx: 5 } => "a",
        Point { x, y } => "b",
    }
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        output.contains("class `Point` has no field `xx`"),
        "unknown field in match-pattern should be diagnosed, got:\n{output}"
    );
    assert!(
        !output.contains("unreachable arm"),
        "unknown class field should not collapse the arm into an irrefutable class pattern, got:\n{output}"
    );
}

/// Ensures an unknown-field arm suppresses usefulness diagnostics.
#[test]
fn class_destructure_unknown_field_arm_does_not_emit_non_exhaustive_match() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class A {
    value int
}

class B {
    value int
}

function f(v: A | B) -> string {
    match (v) {
        A { valeu: 1 } => "a",
        B { value } => "b",
    }
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        output.contains("class `A` has no field `valeu`"),
        "unknown field in match-pattern should be diagnosed, got:\n{output}"
    );
    assert!(
        !output.contains("non-exhaustive match"),
        "invalid match arms should suppress dependent non-exhaustive diagnostics, got:\n{output}"
    );
}

#[test]
fn class_destructure_unknown_field_in_for_binding_reports_field_error() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class Item {
    value int
}

function f(items: Item[]) -> int {
    for (let Item { valeu } in items) {
        valeu
    }
    return 0
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        output.contains("class `Item` has no field `valeu`"),
        "unknown field in for-binding pattern should be diagnosed, got:\n{output}"
    );
    assert!(
        output.contains("Did you mean `value`?"),
        "unknown for-binding field should include typo suggestion, got:\n{output}"
    );
    assert!(
        !output.contains("unresolved name: valeu"),
        "for-binding unknown fields should not cascade into unresolved-name diagnostics, got:\n{output}"
    );
}

#[test]
fn mixed_or_pattern_preserves_partial_expected_type_for_generic_return() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class A {
    field int
}

function produce<T>() -> T {
    throw "boom"
}

function f() -> int {
    let A { field } | [..let field] = produce()
    return field
}
"#,
    );
    let output = render_tir(&db, file);

    // hir_ty's dump renders the call as written (no synthetic turbofish).
    assert!(
        output.contains("produce() : user.A"),
        "mixed OR should preserve the informative class branch as a partial expected type for generic return inference, got:\n{output}"
    );
}
