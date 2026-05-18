//! Snapshot tests for `baml_compiler2_mir`.
//!
//! Each test creates a minimal DB, adds a `.baml` file, lowers all functions
//! to MIR, and snapshots the pretty-printed output.

use std::fmt::Write;

use baml_compiler2_hir::{file_item_tree, loc::FunctionLoc};
use baml_compiler2_mir::{OptLevel, lower_function, pretty::display_function};
use baml_project::ProjectDatabase;

const SNAPSHOT_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/snapshots/compiler2_mir");

fn make_db() -> ProjectDatabase {
    let mut db = ProjectDatabase::new();
    db.set_project_root(std::path::Path::new("."));
    db
}

/// Lower all functions in a file to MIR and pretty-print them.
fn render_mir(db: &ProjectDatabase, file: baml_base::SourceFile) -> String {
    let item_tree = file_item_tree(db, file);
    let mut output = String::new();

    for (local_id, _func_data) in item_tree.functions.iter() {
        let func_loc = FunctionLoc::new(db, file, *local_id);
        let mir = lower_function(db, func_loc, OptLevel::Two);
        writeln!(output, "{}", display_function(&mir)).unwrap();
    }

    output
}

macro_rules! mir_snapshot {
    ($name:expr, $output:expr) => {
        assert_compiler2_snapshot!(SNAPSHOT_PATH, $name, $output);
    };
}

#[test]
fn literal_return() {
    let mut db = make_db();
    let file = db.add_file("test.baml", "function f() -> int { return 42; }");
    mir_snapshot!("literal_return", render_mir(&db, file));
}

#[test]
fn binary_add() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "function f(x: int, y: int) -> int { return x + y; }",
    );
    mir_snapshot!("binary_add", render_mir(&db, file));
}

#[test]
fn if_else() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function f(x: int) -> string {
            if x > 0 {
                return "positive";
            } else {
                return "non-positive";
            }
        }"#,
    );
    mir_snapshot!("if_else", render_mir(&db, file));
}

#[test]
fn let_binding() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        "function f(x: int) -> int { let y = x + 1; return y; }",
    );
    mir_snapshot!("let_binding", render_mir(&db, file));
}

#[test]
fn function_call() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
        function add(a: int, b: int) -> int { return a + b; }
        function f(x: int) -> int { return add(x, 1); }
        "#,
    );
    mir_snapshot!("function_call", render_mir(&db, file));
}

#[test]
fn optional_default_prologue_and_source_omission() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
        function add(base: int, amount: int = base + 2) -> int {
          base + amount
        }

        function main() -> int {
          add(5)
        }
        "#,
    );
    mir_snapshot!(
        "optional_default_prologue_and_source_omission",
        render_mir(&db, file)
    );
}

#[test]
fn optional_named_gap_and_explicit_null() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
        function score(query: string, max_results: int = 10, filter: string? = null) -> int {
          if filter == null {
            max_results
          } else {
            max_results + 1
          }
        }

        function is_null(value: int? = 7) -> bool {
          value == null
        }

        function omitted_middle() -> int {
          score("cats", filter = "recent")
        }

        function explicit_null() -> bool {
          is_null(value = null)
        }
        "#,
    );
    mir_snapshot!(
        "optional_named_gap_and_explicit_null",
        render_mir(&db, file)
    );
}

#[test]
fn optional_named_reordered_args_evaluate_in_source_order() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
        function text(value: string) -> string {
          value
        }

        function number(value: int) -> int {
          value
        }

        function score(query: string, max_results: int = 10, filter: string = "none") -> int {
          max_results
        }

        function main() -> int {
          score(filter = text("first"), query = text("second"), max_results = number(3))
        }
        "#,
    );
    mir_snapshot!(
        "optional_named_reordered_args_evaluate_in_source_order",
        render_mir(&db, file)
    );
}

#[test]
fn optional_dropping_adapter() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
        function combine(x: int, a: int = 10, b: int = 100) -> int {
          x + a + b
        }

        function main() -> int {
          let f: (x: int, b?: int) -> int = combine;
          f(1, b = 5)
        }
        "#,
    );
    mir_snapshot!("optional_dropping_adapter", render_mir(&db, file));
}

#[test]
fn while_loop() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function f(n: int) -> int {
            let sum = 0;
            let i = 0;
            while i < n {
                sum += i;
                i += 1;
            }
            return sum;
        }"#,
    );
    mir_snapshot!("while_loop", render_mir(&db, file));
}

#[test]
fn match_expr() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"function f(x: int) -> string {
            return match x {
                1 => "one",
                2 => "two",
                _ => "other",
            };
        }"#,
    );
    mir_snapshot!("match_expr", render_mir(&db, file));
}

#[test]
fn object_construction() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
        class Point { x int  y int }
        function f() -> Point { return Point { x: 1, y: 2 }; }
        "#,
    );
    mir_snapshot!("object_construction", render_mir(&db, file));
}

#[test]
fn generic_class_destructure_field_projection_uses_instantiated_type() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
        class Box<T> {
            value T
        }

        function f(boxed: Box<int>) -> int {
            let Box<int> { value } = boxed;
            return value;
        }
        "#,
    );
    let output = render_mir(&db, file);
    // Scope the check to `user.f`'s body — auto-derived `to_json` /
    // `from_json` methods on `Box<T>` legitimately have `void` locals
    // because T isn't instantiated in the auto-derive body.
    let f_body = output
        .split("fn user.f(")
        .nth(1)
        .and_then(|tail| tail.split("\nfn ").next())
        .unwrap_or(&output);
    assert!(
        !f_body
            .lines()
            .any(|line| line.trim_start().starts_with("let _") && line.contains(": void")),
        "generic class destructure lowered a projected field through a void local:\n{output}"
    );
}

#[test]
fn match_or_mixed_array_class_binding_uses_branch_local_rest_type() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
        class NumberBag {
            field int[]
        }

        function f(v: NumberBag | int[][]) -> int {
            match (v) {
                NumberBag { field } | [[..let field]: int[], .._] => field[0],
                _ => 0
            }
        }
        "#,
    );
    mir_snapshot!(
        "match_or_mixed_array_class_binding_uses_branch_local_rest_type",
        render_mir(&db, file)
    );
}

#[test]
fn match_or_class_union_field_access_uses_runtime_dispatch() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
        class A { field int }
        class B { field int }
        class C { field int }
        class D { field int }
        class E { field string }

        function f(v: A | B | C | D | E) -> int {
            match (v) {
                A { field: int } | B { field: int } | C { field: int } | D { field: int } => v.field,
                _ => 0
            }
        }
        "#,
    );
    let output = render_mir(&db, file);
    assert!(output.contains("type_tag"), "{output}");
    assert!(output.contains("A:") && output.contains("B:"), "{output}");
    assert!(output.contains("C:") && output.contains("D:"), "{output}");
}

// ─── Phase 4: reflect.type_of concrete types ─────────────────────────────────

/// `reflect.type_of<User>()` should lower to `_N = load_type(Concrete(User))`.
#[test]
fn reflect_type_of_class() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
        class User { name string }
        function f() -> type {
            reflect.type_of<User>()
        }
        "#,
    );
    mir_snapshot!("reflect_type_of_class", render_mir(&db, file));
}

/// `reflect.type_of<int[]>()` — concrete array type.
#[test]
fn reflect_type_of_array() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
        function f() -> type {
            reflect.type_of<int[]>()
        }
        "#,
    );
    mir_snapshot!("reflect_type_of_array", render_mir(&db, file));
}

// ─── Phase 5: reflect.type_of with generic type params ───────────────────────

/// `reflect.type_of<T>()` inside a generic function should lower to
/// `_N = load_type(TypeArgRef(0))`.
#[test]
fn reflect_type_of_bare_typevar() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
        function f<T>() -> type {
            reflect.type_of<T>()
        }
        "#,
    );
    mir_snapshot!("reflect_type_of_bare_typevar", render_mir(&db, file));
}

/// `reflect.type_of<T[]>()` — composite array wrapping a type-var.
/// Should lower to `_N = load_type(Array(TypeArgRef(0)))`.
#[test]
fn reflect_type_of_array_of_typevar() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
        function f<T>() -> type {
            reflect.type_of<T[]>()
        }
        "#,
    );
    mir_snapshot!("reflect_type_of_array_of_typevar", render_mir(&db, file));
}
