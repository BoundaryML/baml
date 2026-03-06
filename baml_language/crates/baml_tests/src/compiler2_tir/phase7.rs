//! Phase 7 tests: Type narrowing.
//!
//! Verifies that type narrowing works correctly for null checks, truthiness,
//! negated conditions, and early-return (diverging then-branch) patterns.

use super::support::{make_db, render_tir};

// ── Null check narrowing: x != null ──────────────────────────────────────────

#[test]
fn narrow_ne_null_then_branch_is_non_nullable() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function f(x: int?) -> int {
  if (x != null) {
    return x;
  }
  return 0;
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(x: int?) -> int {
      { : never
        if (x Ne null : bool) : void
          { : never
            return x : int
          }
        return 0 : 0
      }
    }
    ");
}

#[test]
fn narrow_ne_null_rhs_form() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function f(x: int?) -> int {
  if (null != x) {
    return x;
  }
  return 0;
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(x: int?) -> int {
      { : never
        if (null Ne x : bool) : void
          { : never
            return x : int
          }
        return 0 : 0
      }
    }
    ");
}

// ── Null check narrowing: x == null ──────────────────────────────────────────

#[test]
fn narrow_eq_null_else_branch_is_non_nullable() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function f(x: int?) -> int {
  if (x == null) {
    return 0;
  } else {
    return x;
  }
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(x: int?) -> int {
      { : never
        if (x Eq null : bool) : never
          { : never
            return 0 : 0
          }
        else
          { : never
            return x : int
          }
      }
    }
    ");
}

// ── Truthiness narrowing: if (x) ─────────────────────────────────────────────

#[test]
fn narrow_truthiness_then_branch_non_null() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function f(x: int?) -> int {
  if (x) {
    return x;
  }
  return 0;
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(x: int?) -> int {
      { : never
        if (x : int?) : void
          { : never
            return x : int
          }
        return 0 : 0
      }
    }
    ");
}

// ── Negated narrowing: !(x == null) ──────────────────────────────────────────

#[test]
fn narrow_negated_eq_null_then_branch_non_null() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function f(x: int?) -> int {
  if (!(x == null)) {
    return x;
  }
  return 0;
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(x: int?) -> int {
      { : never
        if (Not x Eq null : bool) : void
          { : never
            return x : int
          }
        return 0 : 0
      }
    }
    ");
}

// ── Early-return narrowing ────────────────────────────────────────────────────

#[test]
fn early_return_null_check_narrows_rest_of_block() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function f(x: int?) -> int {
  if (x == null) {
    return 0;
  }
  return x;
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(x: int?) -> int {
      { : never
        if (x Eq null : bool) : void
          { : never
            return 0 : 0
          }
        return x : int
      }
    }
    ");
}

#[test]
fn early_return_ne_null_check_narrows_rest_of_block() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function f(x: int?) -> int? {
  if (x != null) {
    return x;
  }
  return x;
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(x: int?) -> int? {
      { : never
        if (x Ne null : bool) : void
          { : never
            return x : int
          }
        return x : null
      }
    }
    ");
}

// ── Let-binding captures narrowed type ───────────────────────────────────────

#[test]
fn narrowed_type_captured_in_let_binding() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function f(x: int?) -> int {
  if (x == null) {
    return 0;
  }
  let y = x;
  return y;
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(x: int?) -> int {
      { : never
        if (x Eq null : bool) : void
          { : never
            return 0 : 0
          }
        let y = x : int
        return y : int
      }
    }
    ");
}

// ── Arithmetic on narrowed type ───────────────────────────────────────────────

#[test]
fn narrowed_int_arithmetic_no_error() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function f(x: int?) -> int {
  if (x != null) {
    return x + 1;
  }
  return 0;
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(x: int?) -> int {
      { : never
        if (x Ne null : bool) : void
          { : never
            return x Add 1 : int
          }
        return 0 : 0
      }
    }
    ");
}

// ── Snapshot: full narrowing rendering ───────────────────────────────────────

#[test]
fn snapshot_narrowing_patterns() {
    let mut db = make_db();
    let file = db.add_file(
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
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.f(a: int?, b: string?) -> int {
      { : never
        if (a Eq null : bool) : void
          { : never
            return 0 : 0
          }
        if (b Eq null : bool) : void
          { : never
            return a : int
          }
        let result = a : int
        return result : int
      }
    }
    ");
}

// ── Assignment in narrowed branch ──────────────────────────────────────────────

#[test]
fn assign_wrong_type_in_null_branch_is_error() {
    let mut db = make_db();
    let file = db.add_file(
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
    function user.f(x: int?) -> int {
      { : never
        if (x Eq null : bool) : never
          { : never
            x = "string" : "string"
            return 0 : 0
          }
        else
          { : never
            return x : int
          }
      }
      !! 56..64: type mismatch: expected int?, got "string"
    }
    "#);
}

#[test]
fn assign_method_result_in_null_branch_works() {
    let mut db = make_db();
    let file = db.add_file(
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
    function user.f(x: int?) -> int {
      { : never
        if (x Eq null : bool) : never
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
    "#);
}

// ── String type narrowing ─────────────────────────────────────────────────────

#[test]
fn early_return_string_null_check() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function f(s: string?) -> string {
  if (s == null) {
    return "";
  }
  return s;
}"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.f(s: string?) -> string {
      { : never
        if (s Eq null : bool) : void
          { : never
            return "" : ""
          }
        return s : string
      }
    }
    "#);
}
