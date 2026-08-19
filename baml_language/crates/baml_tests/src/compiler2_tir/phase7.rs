//! Phase 7 tests: Type narrowing.
//!
//! Verifies that type narrowing works correctly for null checks, truthiness,
//! negated conditions, and early-return (diverging then-branch) patterns.

use super::support::{make_db, render_tir};
use crate::engine::TestDbExt;

// ── Null check narrowing: x != null ──────────────────────────────────────────

#[test]
fn narrow_ne_null_then_branch_is_non_nullable() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f(x: int?) -> int {
  if (x != null) {
    return x;
  }
  return 0;
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: int | null) -> int throws never {
      { : never
        if (x != null : bool) : void
          { : never
            return x : int
          }
        return 0 : 0
      }
    }
    block user.f {
    }
    ");
}

#[test]
fn narrow_ne_null_rhs_form() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f(x: int?) -> int {
  if (null != x) {
    return x;
  }
  return 0;
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: int | null) -> int throws never {
      { : never
        if (null != x : bool) : void
          { : never
            return x : int
          }
        return 0 : 0
      }
    }
    block user.f {
    }
    ");
}

// ── Null check narrowing: x == null ──────────────────────────────────────────

#[test]
fn narrow_eq_null_else_branch_is_non_nullable() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f(x: int?) -> int {
  if (x == null) {
    return 0;
  } else {
    return x;
  }
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: int | null) -> int throws never {
      { : never
        if (x == null : bool) : never
          { : never
            return 0 : 0
          }
        else
          { : never
            return x : int
          }
      }
    }
    block user.f {
    }
    block user.f {
    }
    ");
}

// ── Truthiness narrowing: if (x) ─────────────────────────────────────────────

#[test]
fn narrow_truthiness_then_branch_non_null() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f(x: int?) -> int {
  if (x) {
    return x;
  }
  return 0;
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: int | null) -> int throws never {
      { : never
        if (x : int | null) : void
          { : never
            return x : int | null
          }
        return 0 : 0
      }
      !! 35..36: type mismatch: expected bool, got int | null
      !! 51..52: type mismatch: expected int, got int | null
    }
    block user.f {
    }
    ");
}

// ── Negated narrowing: !(x == null) ──────────────────────────────────────────

#[test]
fn narrow_negated_eq_null_then_branch_non_null() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f(x: int?) -> int {
  if (!(x == null)) {
    return x;
  }
  return 0;
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: int | null) -> int throws never {
      { : never
        if (Not x == null : bool) : void
          { : never
            return x : int
          }
        return 0 : 0
      }
    }
    block user.f {
    }
    ");
}

// ── Early-return narrowing ────────────────────────────────────────────────────

#[test]
fn early_return_null_check_narrows_rest_of_block() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f(x: int?) -> int {
  if (x == null) {
    return 0;
  }
  return x;
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: int | null) -> int throws never {
      { : never
        if (x == null : bool) : void
          { : never
            return 0 : 0
          }
        return x : int
      }
    }
    block user.f {
    }
    ");
}

#[test]
fn early_return_ne_null_check_narrows_rest_of_block() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f(x: int?) -> int? {
  if (x != null) {
    return x;
  }
  return x;
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: int | null) -> int | null throws never {
      { : never
        if (x != null : bool) : void
          { : never
            return x : int
          }
        return x : null
      }
    }
    block user.f {
    }
    ");
}

// ── Let-binding captures narrowed type ───────────────────────────────────────

#[test]
fn narrowed_type_captured_in_let_binding() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f(x: int?) -> int {
  if (x == null) {
    return 0;
  }
  let y = x;
  return y;
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: int | null) -> int throws never {
      { : never
        if (x == null : bool) : void
          { : never
            return 0 : 0
          }
        let y = x : int
        return y : int
      }
    }
    block user.f {
    }
    ");
}

// ── Arithmetic on narrowed type ───────────────────────────────────────────────

#[test]
fn narrowed_int_arithmetic_no_error() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f(x: int?) -> int {
  if (x != null) {
    return x + 1;
  }
  return 0;
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(x: int | null) -> int throws never {
      { : never
        if (x != null : bool) : void
          { : never
            return x + 1 : int
          }
        return 0 : 0
      }
    }
    block user.f {
    }
    ");
}

// ── Snapshot: full narrowing rendering ───────────────────────────────────────

#[test]
fn snapshot_narrowing_patterns() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f(a: int?, b: string?) -> int {
  if (a == null) {
    return 0;
  }
  if (b == null) {
    return a;
  }
  let result = a;
  return result;
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.f(a: int | null, b: string | null) -> int throws never {
      { : never
        if (a == null : bool) : void
          { : never
            return 0 : 0
          }
        if (b == null : bool) : void
          { : never
            return a : int
          }
        let result = a : int
        return result : int
      }
    }
    block user.f {
    }
    block user.f {
    }
    ");
}

// ── Assignment in narrowed branch ──────────────────────────────────────────────

#[test]
fn assign_wrong_type_in_null_branch_is_error() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f(x: int?) -> int {
  if (x == null) {
    x = "string";
    return 0;
  } else {
    return x;
  }
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f(x: int | null) -> int throws never {
      { : never
        if (x == null : bool) : never
          { : never
            x = "string" : "string"
            return 0 : 0
          }
        else
          { : never
            return x : int
          }
      }
      !! 56..64: type mismatch: expected int | null, got "string"
    }
    block user.f {
    }
    block user.f {
    }
    "#);
}

#[test]
fn assign_method_result_in_null_branch_works() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f(x: int?) -> int {
  if (x == null) {
    x = "string".length();
    return 0;
  } else {
    return x;
  }
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f(x: int | null) -> int throws never {
      { : never
        if (x == null : bool) : never
          { : never
            x = "string".length() : int
            return 0 : 0
          }
        else
          { : never
            return x : int
          }
      }
    }
    block user.f {
    }
    block user.f {
    }
    "#);
}

#[test]
fn assignment_before_shadow_survives_scope_restore() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f(x: int?) -> int {
  {
    x = 1;
    let x: string = "shadow";
    x;
  };
  return x;
}"#,
    );
    let output = render_tir(&db, file);
    assert!(
        output.contains("return x : 1"),
        "outer assignment before inner shadow should remain visible after block:\n{output}"
    );
    assert!(
        !output.contains("type mismatch"),
        "assignment before shadow should satisfy the int return type:\n{output}"
    );
}

#[test]
fn inner_declared_type_does_not_leak_after_shadow() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f() -> int {
  let x: int = 1;
  {
    let x: string = "shadow";
    x;
  };
  x = 2;
  return x;
}"#,
    );
    let output = render_tir(&db, file);
    assert!(
        output.contains("x = 2 : 2"),
        "outer declared type should be restored after inner typed shadow:\n{output}"
    );
    assert!(
        !output.contains("type mismatch"),
        "inner declared type metadata should not constrain outer assignment:\n{output}"
    );
}

#[test]
fn assignment_uses_declared_type_after_narrowing() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f() -> int? {
  let x: int? = 1;
  if (x != null) {
    x = null;
  };
  return x;
}"#,
    );
    let output = render_tir(&db, file);
    assert!(
        output.contains("x = null : null"),
        "assignment should be checked against the declared optional type after narrowing:\n{output}"
    );
    assert!(
        !output.contains("type mismatch"),
        "narrowed current type should not become the assignment contract:\n{output}"
    );
}

#[test]
fn unannotated_inner_shadow_masks_outer_declared_type() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f() -> int {
  let x: int = 1;
  {
    let x = "shadow";
    x = "updated";
  };
  return x;
}"#,
    );
    let output = render_tir(&db, file);
    assert!(
        !output.contains("type mismatch"),
        "unannotated inner shadow should not be checked against outer annotation:\n{output}"
    );
}

#[test]
fn early_return_narrowing_inside_nested_block_does_not_leak() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f(x: int?) -> int? {
  {
    if (x == null) {
      return 0;
    }
  };
  return x;
}"#,
    );
    let output = render_tir(&db, file);
    // hir_ty's flow narrowing is PATH-SENSITIVE across sequential blocks
    // (rustc-style): the nested block unconditionally returns when `x`
    // is null, so afterwards `x` provably isn't - a strictly stronger,
    // sound refinement TIR scoped away.
    assert!(
        output.contains("return x : int"),
        "the early return proves `x` non-null for the rest of the body:\n{output}"
    );
}

// ── String type narrowing ─────────────────────────────────────────────────────

#[test]
fn early_return_string_null_check() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"function f(s: string?) -> string {
  if (s == null) {
    return "";
  }
  return s;
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f(s: string | null) -> string throws never {
      { : never
        if (s == null : bool) : void
          { : never
            return "" : ""
          }
        return s : string
      }
    }
    block user.f {
    }
    "#);
}

#[test]
fn captured_local_is_not_narrowed() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class Foo { field: int }

function f(x: Foo | int) -> int {
  let task = spawn { x = 0; };
  match (x) {
    Foo => x.field,
    int => x,
  }
}
"#,
    );
    let output = render_tir(&db, file);
    // hir_ty types the failed member access with the ERROR sentinel
    // (replace-with-error, not TIR's `unknown`) and reports the member
    // error; the capture still blocks narrowing.
    assert!(
        output.contains("x.field : !error")
            && output.contains("type `int | Foo` has no member `field`"),
        "captured local must retain its declared union type:\n{output}"
    );
}

#[test]
fn captured_local_is_not_narrowed_by_condition() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(x: int?) -> int {
  let task = spawn { x = null; };
  if (x != null) {
    return x;
  }
  0
}
"#,
    );
    let output = render_tir(&db, file);
    assert!(
        output.contains("return x : int | null") && output.contains("expected int, got int | null"),
        "captured local must not narrow across a condition:\n{output}"
    );
}

#[test]
fn uncaptured_local_is_narrowed() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class Foo { field: int }

function f(x: Foo | int) -> int {
  match (x) {
    Foo => x.field,
    int => x,
  }
}
"#,
    );
    let output = render_tir(&db, file);
    assert!(
        output.contains("x.field : int") && output.contains("x : int"),
        "uncaptured local should narrow in each match arm:\n{output}"
    );
    assert!(!output.contains("!!"), "unexpected diagnostics:\n{output}");
}

#[test]
fn field_is_not_narrowed() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class Foo { field: int }
class Box { value: Foo | int }

function f(box: Box) -> int {
  match (box.value) {
    Foo => box.value.field,
    int => box.value,
  }
}
"#,
    );
    let output = render_tir(&db, file);
    // Error-sentinel typing as above; the field scrutinee stays unnarrowed.
    assert!(
        output.contains("box.value.field : !error")
            && output.contains("type `int | Foo` has no member `field`"),
        "field access must retain its declared union type:\n{output}"
    );
}

#[test]
fn uncaptured_snapshot_of_captured_local_is_narrowed() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class Foo { field: int }

function f(x: Foo | int) -> int {
  let task = spawn { x = 0; };
  let snapshot = x;
  match (snapshot) {
    Foo => snapshot.field,
    int => snapshot,
  }
}
"#,
    );
    let output = render_tir(&db, file);
    assert!(
        output.contains("snapshot.field : int") && output.contains("snapshot : int"),
        "uncaptured snapshot should narrow even when its source is captured:\n{output}"
    );
    assert!(!output.contains("!!"), "unexpected diagnostics:\n{output}");
}

#[test]
fn destructured_field_local_is_narrowed() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class Bar { value: int }
class Foo { field: Bar | int }

function f(x: Foo | int) -> int {
  match (x) {
    Foo => {
      let Foo { field } = x;
      match (field) {
        Bar => field.value,
        int => field,
      }
    },
    int => x,
  }
}
"#,
    );
    let output = render_tir(&db, file);
    assert!(
        output.contains("field.value : int") && output.contains("field : int"),
        "destructured field local should narrow in each match arm:\n{output}"
    );
    assert!(!output.contains("!!"), "unexpected diagnostics:\n{output}");
}
