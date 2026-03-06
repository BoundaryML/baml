//! Phase 6 tests: Generic type variable binding and builtin method resolution.
//!
//! Verifies that `Ty::List`, `Ty::Map`, and `Ty::Primitive(String)` correctly
//! resolve methods to the builtin `.baml` stub declarations with type variable
//! substitution applied.

use super::support::{make_db, render_tir};

// ── Array method resolution ───────────────────────────────────────────────────

#[test]
fn array_length_returns_int() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "function f(arr: int[]) -> int { return arr.length(); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(arr: int[]) -> int {
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
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(arr: int[]) -> int? {
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
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(arr: string[]) -> string? {
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
    function user.f(arr: string[]) -> string {
      { : never
        return arr.join(",") : string
      }
    }
    "#);
}

// ── Map method resolution ─────────────────────────────────────────────────────

#[test]
fn map_keys_returns_key_type_array() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "function f(m: map<string, int>) -> string[] { return m.keys(); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(m: map<string, int>) -> string[] {
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
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(m: map<string, int>) -> int[] {
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
    function user.f(m: map<string, int>) -> bool {
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
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(m: map<string, int>) -> int {
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
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(s: string) -> int {
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
    function user.f(s: string) -> string[] {
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
    function user.f(s: string) -> bool {
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
        "function f(s: string) -> string { return s.toLowerCase(); }",
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(s: string) -> string {
      { : never
        return s.toLowerCase() : string
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
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(arr: int[]) -> int {
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
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(arr: int[]) -> int? {
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
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(m: map<string, int>) -> string[] {
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
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(img: image) -> string? {
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
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(img: image) -> string {
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
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(img: image) -> string? {
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
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(doc: pdf) -> string? {
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
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(a: audio) -> string {
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
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(v: video) -> string? {
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
        output.contains("unresolved member"),
        "Expected 'unresolved member' in output, got:\n{output}"
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
    function user.f() -> image {
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
    function user.f() -> pdf {
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
    function user.f() -> audio {
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
    function user.f() -> video {
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
        output.contains("unresolved member"),
        "Expected 'unresolved member' in output, got:\n{output}"
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
        output.contains("unresolved member"),
        "Expected 'unresolved member' in output, got:\n{output}"
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
        output.contains("unresolved member"),
        "Expected 'unresolved member' in output, got:\n{output}"
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
