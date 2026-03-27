//! Snapshot tests for `baml_compiler2_mir`.
//!
//! Each test creates a minimal DB, adds a `.baml` file, lowers all functions
//! to MIR, and snapshots the pretty-printed output.

use std::fmt::Write;

use baml_compiler2_hir::{file_item_tree, loc::FunctionLoc};
use baml_compiler2_mir::{lower_function, pretty::display_function};
use baml_project::ProjectDatabase;
use insta::{assert_snapshot, with_settings};

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
        let mir = lower_function(db, func_loc);
        writeln!(output, "{}", display_function(&mir)).unwrap();
    }

    output
}

macro_rules! mir_snapshot {
    ($name:expr, $output:expr) => {
        with_settings!({ snapshot_path => SNAPSHOT_PATH, omit_expression => true }, {
            assert_snapshot!($name, $output);
        });
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
