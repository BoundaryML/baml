//! Phase 6 tests: Generic type variable binding and builtin method resolution.
//!
//! Verifies that `Ty::List`, `Ty::Map`, and `Ty::Primitive(String)` correctly
//! resolve methods to the builtin `.baml` stub declarations with type variable
//! substitution applied.

use super::support::{expr_type_in_function, make_db, render_tir};

// ── Array method resolution ───────────────────────────────────────────────────

#[test]
fn array_length_returns_int() {
    let mut db = make_db();
    let file = db.add_file(
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
    let file = db.add_file(
        "test.baml",
        "function f(arr: int[]) -> int? { return arr.at(0); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(arr: int[]) -> int? throws never {
      { : never
        return arr.at(0) : int?
      }
    }
    ");
}

#[test]
fn array_at_returns_element_type_string() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "function f(arr: string[]) -> string? { return arr.at(0); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(arr: string[]) -> string? throws never {
      { : never
        return arr.at(0) : string?
      }
    }
    ");
}

#[test]
fn array_join_returns_string() {
    let mut db = make_db();
    let file = db.add_file(
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
    let file = db.add_file(
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
        tir.contains("type mismatch: expected user.Array<int>, got int[]"),
        "expected nominal user.Array<T> to stay distinct from builtin int[], got:\n{tir}"
    );
}

// ── Map method resolution ─────────────────────────────────────────────────────

#[test]
fn map_keys_returns_key_type_array() {
    let mut db = make_db();
    let file = db.add_file(
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
    let file = db.add_file(
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
    let file = db.add_file(
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
    let file = db.add_file(
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
    let file = db.add_file(
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
    let file = db.add_file(
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
    let file = db.add_file(
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
    let file = db.add_file(
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
    let file = db.add_file(
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
    let file = db.add_file(
        "test.baml",
        "function f(arr: int[]) -> int? { let x = arr.at(0); return x; }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(arr: int[]) -> int? throws never {
      { : never
        let x = arr.at(0) : int?
        return x : int?
      }
    }
    ");
}

#[test]
fn let_inferred_from_map_keys() {
    let mut db = make_db();
    let file = db.add_file(
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
    let file = db.add_file(
        "test.baml",
        "function f(img: image) -> string? { return img.url(); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(img: image) -> string? throws never {
      { : never
        return img.url() : string?
      }
    }
    ");
}

#[test]
fn image_base64_returns_string() {
    let mut db = make_db();
    let file = db.add_file(
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
    let file = db.add_file(
        "test.baml",
        "function f(img: image) -> string? { return img.mime_type(); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(img: image) -> string? throws never {
      { : never
        return img.mime_type() : string?
      }
    }
    ");
}

#[test]
fn pdf_url_returns_optional_string() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "function f(doc: pdf) -> string? { return doc.url(); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(doc: pdf) -> string? throws never {
      { : never
        return doc.url() : string?
      }
    }
    ");
}

#[test]
fn audio_base64_returns_string() {
    let mut db = make_db();
    let file = db.add_file(
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
    let file = db.add_file(
        "test.baml",
        "function f(v: video) -> string? { return v.file(); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(v: video) -> string? throws never {
      { : never
        return v.file() : string?
      }
    }
    ");
}

#[test]
fn image_missing_method_produces_unresolved_member() {
    let mut db = make_db();
    let file = db.add_file(
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
    let file = db.add_file(
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
    let file = db.add_file(
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
    let file = db.add_file(
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
    let file = db.add_file(
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
    let file = db.add_file(
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
    let file = db.add_file(
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
    let file = db.add_file(
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
    let file = db.add_file(
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
    let file = db.add_file(
        "test.baml",
        r#"
function f(callback: ((x: int) -> int)?) -> int? {
    return callback?.(42)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f(callback: ((x: int) -> int throws never)?) -> int? throws never {
      { : never
        return callback?.(42) : int?
      }
    }
    "#);
}

#[test]
fn optional_call_generic_map() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function f(arr: int[]?) -> int[]? {
    return arr?.map?.((x: int) -> int { x + 1 })
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(arr: int[]?) -> int[]? throws never {
      { : never
        return arr?.map?.((x: int) -> int { ... }) : int[]?
      }
    }
    lambda user.f {
    }
    ");
}

#[test]
fn direct_optional_method_call_generic_map() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function f(arr: int[]?) -> int[]? {
    return arr?.map((x) -> { x + 1 })
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f(arr: int[]?) -> int[]? throws never {
      { : never
        return arr?.map((x) -> { ... }) : int[]?
      }
    }
    lambda user.f {
    }
    "#);
}

#[test]
fn optional_call_arg_type_checking() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function f(callback: ((x: int) -> int)?) -> int? {
    return callback?.("wrong")
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f(callback: ((x: int) -> int throws never)?) -> int? throws never {
      { : never
        return callback?.("wrong") : int?
      }
      !! 74..81: type mismatch: expected int, got "wrong"
    }
    "#);
}

#[test]
fn optional_call_checks_higher_order_function_arguments() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function demo(cb: (((x: int) -> int) -> int)?) -> int? throws never {
  let risky = (x: int) -> int throws string {
    throw "boom"
  }

  cb?.(risky)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.demo(cb: (((x: int) -> int throws never) -> int throws never)?) -> int? throws never {
      { : int?
        let risky = : (x: int) -> int throws string
          (x: int) -> int throws string { ... } : (x: int) -> int throws string
            {
              throw "boom"
            }
        cb?.(risky) : int?
      }
      !! 146..151: type mismatch: expected (x: int) -> int throws never, got (x: int) -> int throws string
    }
    lambda user.demo {
    }
    "#);
}

#[test]
fn optional_call_through_type_alias() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
type MaybeFn = ((x: int) -> int)?
function f(callback: MaybeFn) -> int? {
    return callback?.(42)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    type user.MaybeFn = ((x: int) -> int throws never)?
    function user.f(callback: user.MaybeFn) -> int? throws never {
      { : never
        return callback?.(42) : int?
      }
    }
    type user.MaybeFn$stream = null | unknown
    ");
}

#[test]
fn optional_field_access_through_type_alias() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
class User { name string }
type MaybeUser = User?
function f(u: MaybeUser) -> string? {
    return u?.name
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    class user.User {
      name: string
    }
    function user.User.to_json(self: user.User) -> baml.json.json throws baml.json.JsonSerializationError | baml.json.JsonParseError {
      map { "name": self.name.to_json() } : map<string, baml.json.json>
    }
    function user.User.from_json(j: baml.json.json) -> user.User throws baml.json.JsonParseError | baml.json.JsonDecodeError {
      User { name: baml.json.from_json<string>(baml.json.field(j, "name")) } : user.User
    }
    type user.MaybeUser = user.User?
    function user.f(u: user.MaybeUser) -> string? throws never {
      { : never
        return u?.name : string?
      }
    }
    class user.User$stream {
      name: null | string
    }
    type user.MaybeUser$stream = null | user.User$stream
    "#);
}

#[test]
fn optional_index_through_type_alias() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
type MaybeInts = int[]?
function f(xs: MaybeInts) -> int? {
    return xs?.[0]
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    type user.MaybeInts = int[]?
    function user.f(xs: user.MaybeInts) -> int? throws never {
      { : never
        return xs?.[0] : int?
      }
    }
    type user.MaybeInts$stream = null | int[]
    "#);
}

#[test]
fn optional_call_expected_nonoptional_still_mismatches() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function f(cb: ((x: int) -> int)?) -> int {
    return cb?.(1)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f(cb: ((x: int) -> int throws never)?) -> int throws never {
      { : never
        return cb?.(1) : int?
      }
      !! 56..63: type mismatch: expected int, got int?
    }
    "#);
}

#[test]
fn optional_call_nullable_return_preserves_phase0() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function f(cb: ((x: int) -> string?)?) -> string? {
    return cb?.(1)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f(cb: ((x: int) -> string? throws never)?) -> string? throws never {
      { : never
        return cb?.(1) : string?
      }
    }
    "#);
}

#[test]
fn optional_call_null_short_circuit_still_checks_args() {
    let mut db = make_db();
    let file = db.add_file(
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
    let file = db.add_file(
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
    let file = db.add_file(
        "test.baml",
        r#"
function f() -> int {
    let cb = null
    return cb?.(1)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f() -> int throws never {
      { : never
        let cb = null : null
        return cb?.(1) : null
      }
      !! 52..59: type mismatch: expected int, got null
    }
    "#);
}

#[test]
fn optional_push_establishes_element_type() {
    let mut db = make_db();
    let file = db.add_file(
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
        let xs = [] : never[] -> int[] (evolving)
        xs?.push?.(1) : int?
        xs.push("a") : int
        return null : null
      }
      !! 44..52: did you mean `xs.push`? `xs?.push` is unnecessary, because `xs` cannot be null
      !! 70..73: type mismatch: expected int, got string
    }
    "#);
}

#[test]
fn direct_optional_push_establishes_element_type() {
    let mut db = make_db();
    let file = db.add_file(
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
        let xs = [] : never[] -> int[] (evolving)
        xs?.push(1) : int?
        xs.push("a") : int
        return null : null
      }
      !! 44..52: did you mean `xs.push`? `xs?.push` is unnecessary, because `xs` cannot be null
      !! 68..71: type mismatch: expected int, got string
    }
    "#);
}

#[test]
fn optional_push_returns_optional_int() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function f(xs: int[]?) -> int? {
    return xs?.push?.(2)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f(xs: int[]?) -> int? throws never {
      { : never
        return xs?.push?.(2) : int?
      }
    }
    "#);
}

#[test]
fn push_establishment_updates_let_binding_type_for_function_values() {
    let mut db = make_db();
    let file = db.add_file(
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
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.cb() -> int throws never {
      { : never
        return 1 : 1
      }
    }
    function user.f() -> null throws never {
      { : never
        let callbacks = [] : never[] -> (() -> int throws never)[] (evolving)
        callbacks.push(cb) : int
        return null : null
      }
    }
    ");
}

#[test]
fn optional_push_inner_callee_stays_optional_callable() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function f(xs: int[]?) -> int? {
    return xs?.push?.(2)
}
"#,
    );
    assert_eq!(
        expr_type_in_function(&db, file, "f", "xs?.push"),
        "((self: int[], item: int) -> int throws never)?"
    );
}

#[test]
fn named_wrapper_value_preserves_effect_param() {
    let mut db = make_db();
    let file = db.add_file(
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
        "(cb: (x: int) -> string throws __effect_param_0) -> string throws __effect_param_0"
    );
}

#[test]
fn named_wrapper_value_that_catches_callback_has_never_callable_throws() {
    let mut db = make_db();
    let file = db.add_file(
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
        "(cb: (x: int) -> string throws __effect_param_0) -> string throws never"
    );
}

#[test]
fn returned_wrapper_value_preserves_explicit_callback_throws() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function forward(cb: (x: int) -> int throws string) -> int {
    return cb(1)
}

function make() -> ((cb: (x: int) -> int throws string) -> int) {
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
    let file = db.add_file(
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
        "(cb: () -> int throws string) -> int throws string"
    );
}

#[test]
fn bound_method_value_preserves_declared_throws() {
    let mut db = make_db();
    let file = db.add_file(
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
    let file = db.add_file(
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
        ty.starts_with("(n: int) -> string throws "),
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
    let file = db.add_file(
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
        "(self: int[], f: (int) -> U throws E) -> U[] throws E"
    );
}

#[test]
fn wrapper_value_around_builtin_map_preserves_callback_surface() {
    let mut db = make_db();
    let file = db.add_file(
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
        "(cb: (value: int) -> int throws __effect_param_0, values: int[]) -> int[] throws __effect_param_0"
    );
}

#[test]
fn lambda_value_preserves_explicit_throws() {
    let mut db = make_db();
    let file = db.add_file(
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
fn stored_lambda_with_omitted_throws_stays_closed_in_expr_type() {
    let mut db = make_db();
    let file = db.add_file(
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
    assert_eq!(
        expr_type_in_function(&db, file, "f", "risky"),
        "(x: int) -> int throws never"
    );
}

#[test]
fn returned_triple_nested_lambda_reads_cleanly() {
    let mut db = make_db();
    let file = db.add_file(
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
    let file = db.add_file(
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
    let file = db.add_file(
        "test.baml",
        r#"
function f(xs: int[]) -> string {
    return xs.push(1)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f(xs: int[]) -> string throws never {
      { : never
        return xs.push(1) : int
      }
      !! 45..56: type mismatch: expected string, got int
    }
    "#);
}

#[test]
fn optional_push_fast_path_still_checked_against_expected() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function f(xs: int[]?) -> string {
    return xs?.push?.(1)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f(xs: int[]?) -> string throws never {
      { : never
        return xs?.push?.(1) : int?
      }
      !! 46..60: type mismatch: expected string, got int?
    }
    "#);
}

#[test]
fn optional_call_lambda_contextual_typing() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function f(arr: int[]?) -> int[]? {
    return arr?.map?.((x) -> { x + 1 })
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(arr: int[]?) -> int[]? throws never {
      { : never
        return arr?.map?.((x) -> { ... }) : int[]?
      }
    }
    lambda user.f {
    }
    ");
}

#[test]
fn optional_call_lambda_with_explicit_types() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function f(arr: int[]?) -> int[]? {
    return arr?.map?.((x: int) -> int { x + 1 })
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(arr: int[]?) -> int[]? throws never {
      { : never
        return arr?.map?.((x: int) -> int { ... }) : int[]?
      }
    }
    lambda user.f {
    }
    ");
}

#[test]
fn optional_call_builtin_string_method() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function f(s: string?) -> string[]? {
    return s?.split?.(",")
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f(s: string?) -> string[]? throws never {
      { : never
        return s?.split?.(",") : string[]?
      }
    }
    "#);
}

#[test]
fn optional_call_builtin_map_method() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function f(m: map<string, int>?) -> string[]? {
    return m?.keys?.()
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f(m: map<string, int>?) -> string[]? throws never {
      { : never
        return m?.keys?.() : string[]?
      }
    }
    "#);
}

#[test]
fn optional_call_unnecessary_chaining_diagnostic() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function f(callback: (x: int) -> int) -> int? {
    return callback?.(42)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f(callback: (x: int) -> int throws never) -> int? throws never {
      { : never
        return callback?.(42) : int?
      }
      !! 60..74: did you mean `callback(42)`? `callback?.(42)` is unnecessary, because `callback` cannot be null
    }
    "#);
}

#[test]
fn parenthesized_optional_method_call_breaks_chain() {
    let mut db = make_db();
    let file = db.add_file(
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
    assert!(
        output.contains("return u?.getName() : unknown"),
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
    let file = db.add_file(
        "test.baml",
        r#"
function f(callback: ((x: int) -> int)?) -> int? {
    return callback?.(1, 2)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f(callback: ((x: int) -> int throws never)?) -> int? throws never {
      { : never
        return callback?.(1, 2) : int?
      }
      !! 63..79: expected 1 argument(s), got 2
    }
    "#);
}

#[test]
fn index_assignment_establishment_updates_let_binding_type() {
    let mut db = make_db();
    let file = db.add_file(
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

    assert!(
        output.contains("let xs = [] : never[] -> int[] (evolving)"),
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
    let file = db.add_file(
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

    assert!(
        output.contains("let xs = [] : never[] -> int[] (evolving)"),
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
    let file = db.add_file(
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

    assert!(
        output.contains("let xs = [] : never[] -> string[] (evolving)"),
        "expected xs to be established by the first push in the loop body, got:\n{output}"
    );
    assert!(
        output.contains("type mismatch: expected string, got int"),
        "post-loop push should be checked against the loop-established element type, got:\n{output}"
    );
}

#[test]
fn array_rest_binding_annotation_must_be_array_type() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function f() -> int {
    let [..let rest: int] = [1, 2]
    return 0
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        output.contains("rest pattern `..` cannot carry a sub-pattern"),
        "rest with sub-pattern should be rejected (only bare `..` allowed), got:\n{output}"
    );
}

#[test]
fn array_rest_binding_array_annotation_is_valid() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function f() -> int {
    let [..let rest: int[]] = [1, 2]
    return rest[0]
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        output.contains("let [..rest: int[]] = [1, 2] : int[]"),
        "rest pattern annotation should be checked against the rest slice type, got:\n{output}"
    );
    assert!(
        !output.contains("type mismatch"),
        "array rest annotation should be valid, got:\n{output}"
    );
}

#[test]
fn array_rest_cannot_use_class_destructure_for_rest_slice() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
class Box {
    value int
}

function f(boxes: Box[]) -> int {
    let [..Box { value }] = boxes
    return value
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        output.contains("rest pattern `..` cannot carry a sub-pattern"),
        "rest with sub-pattern should be rejected (only bare `..` allowed), got:\n{output}"
    );
}

#[test]
fn array_nested_rest_annotation_must_match_rest_slice() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function f(xs: int[]) -> int {
    match (xs) {
        [..[let x]: int] => x,
        _ => 0
    }
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        output.contains("rest pattern `..` cannot carry a sub-pattern"),
        "rest with sub-pattern should be rejected (only bare `..` allowed), got:\n{output}"
    );
}

#[test]
fn nested_refutable_array_under_rest_is_rejected_in_let() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function f(xs: int[]) -> int {
    let [..[let x]] = xs
    return x
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        output.contains("refutable pattern in let binding"),
        "nested exact array under rest should still make the let refutable, got:\n{output}"
    );
}

#[test]
fn refutable_array_pattern_is_rejected_in_for_binding() {
    let mut db = make_db();
    let file = db.add_file(
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
    let file = db.add_file(
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
fn mixed_or_pattern_preserves_partial_expected_type_for_generic_return() {
    let mut db = make_db();
    let file = db.add_file(
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

    assert!(
        output.contains("produce<T>() : user.A"),
        "mixed OR should preserve the informative class branch as a partial expected type for generic return inference, got:\n{output}"
    );
}
