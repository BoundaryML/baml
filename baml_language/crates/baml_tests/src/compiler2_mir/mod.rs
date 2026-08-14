//! Snapshot tests for `baml_compiler2_mir`.
//!
//! Each test creates a minimal DB, adds a `.baml` file, lowers all functions
//! to MIR, and snapshots the pretty-printed output.

use std::fmt::Write;

use baml_compiler2_mir::{
    MirFunctionKind, OptLevel, Terminator, lower_function, pretty::display_function,
};
use baml_compiler2_ppir::item_data::{file_functions, function_data, function_source_map};
use baml_project::ProjectDatabase;

const SNAPSHOT_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/snapshots/compiler2_mir");

fn make_db() -> ProjectDatabase {
    let mut db = ProjectDatabase::new();
    db.set_project_root(std::path::Path::new("."));
    db
}

/// Lower all functions in a file to MIR and pretty-print them.
fn render_mir(db: &ProjectDatabase, file: baml_base::SourceFile) -> String {
    // Dump in source order (by declaration span) — intrinsic and
    // salsa-enumeration-independent, matching the generated `test_04_5_mir`.
    let mut functions = file_functions(db, file).to_vec();
    functions.sort_by_key(|loc| function_source_map(db, *loc).span.start());
    let mut output = String::new();

    for func_loc in functions {
        let mir = lower_function(db, func_loc, OptLevel::Two);
        writeln!(output, "{}", display_function(&mir)).unwrap();
    }

    output
}

#[test]
fn explicit_local_id_reaches_direct_and_sysop_mir_terminators() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function leaf(n: int) -> int { n }

function main(call_id: boundary.LocalId, sysop_id: boundary.LocalId) -> int throws baml.errors.Io {
  let value = leaf(1, $id = call_id)
  baml.sys.sleep(baml.time.Duration.from_milliseconds(0n), $id = sysop_id)
  value
}
"#,
    );
    let main_loc = *file_functions(&db, file)
        .iter()
        .find(|&&loc| function_data(&db, loc).name.as_str() == "main")
        .expect("main function");
    let mir = lower_function(&db, main_loc, OptLevel::Two);
    let MirFunctionKind::Bytecode(body) = &mir.kind else {
        panic!("main must lower to bytecode")
    };

    assert!(body.blocks.iter().any(|block| matches!(
        block.terminator,
        Some(Terminator::Call {
            runtime_id: Some(_),
            ..
        })
    )));
    assert!(body.blocks.iter().any(|block| matches!(
        block.terminator,
        Some(Terminator::SysOp {
            runtime_id: Some(_),
            ..
        })
    )));
}

#[test]
fn explicit_local_id_reaches_indirect_optional_virtual_and_union_calls() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
interface Speaker {
  function speak(self) -> int throws never
}

class Dog {
  function speak(self) -> int { 1 }
}

class Cat {
  function speak(self) -> int { 2 }
}

implements Speaker for Dog {}
implements Speaker for Cat {}

function indirect(callback: (int) -> int throws never, id: boundary.LocalId) -> int {
  callback(1, $id = id)
}

function optional(callback: ((int) -> int throws never)?, id: boundary.LocalId) -> int? {
  callback?.(1, $id = id)
}

function virtual(speaker: Speaker, id: boundary.LocalId) -> int {
  speaker.speak($id = id)
}

function union_dispatch(speaker: Dog | Cat, id: boundary.LocalId) -> int {
  speaker.speak($id = id)
}
"#,
    );
    let lower_named = |name: &str| {
        let loc = *file_functions(&db, file)
            .iter()
            .find(|&&loc| function_data(&db, loc).name.as_str() == name)
            .unwrap_or_else(|| panic!("{name} function"));
        lower_function(&db, loc, OptLevel::Two)
    };

    for name in ["indirect", "optional"] {
        let mir = lower_named(name);
        let MirFunctionKind::Bytecode(body) = &mir.kind else {
            panic!("{name} must lower to bytecode")
        };
        assert!(
            body.blocks.iter().any(|block| matches!(
                block.terminator,
                Some(Terminator::Call {
                    runtime_id: Some(_),
                    ..
                })
            )),
            "{name} dropped its runtime ID: {}",
            display_function(&mir)
        );
    }

    let virtual_mir = lower_named("virtual");
    let MirFunctionKind::Bytecode(virtual_body) = &virtual_mir.kind else {
        panic!("virtual must lower to bytecode")
    };
    assert!(
        virtual_body.blocks.iter().any(|block| matches!(
            block.terminator,
            Some(Terminator::VirtualCall {
                runtime_id: Some(_),
                ..
            })
        )),
        "virtual call dropped its runtime ID: {}",
        display_function(&virtual_mir)
    );

    let union_mir = lower_named("union_dispatch");
    let MirFunctionKind::Bytecode(union_body) = &union_mir.kind else {
        panic!("union_dispatch must lower to bytecode")
    };
    let union_calls = union_body
        .blocks
        .iter()
        .filter_map(|block| match block.terminator.as_ref() {
            Some(Terminator::Call { runtime_id, .. }) => Some(runtime_id),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        union_calls.len() >= 2,
        "expected one dispatch call per union member"
    );
    assert!(
        union_calls.iter().all(|runtime_id| runtime_id.is_some()),
        "a union dispatch branch dropped its runtime ID: {}",
        display_function(&union_mir)
    );
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
            return match (x) {
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

#[test]
fn source_param_interface_dispatch_respects_shadowed_local_binding() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
        class Shadow {
            function iter(self) -> string {
                "shadow"
            }
        }

        function f(source: baml.iter.Iterable<Item = int, Error = never>) -> string {
            let source = Shadow {};
            source.iter()
        }
        "#,
    );
    let output = render_mir(&db, file);
    let f_body = output
        .split("fn user.f(")
        .nth(1)
        .and_then(|tail| tail.split("\nfn ").next())
        .unwrap_or(&output);
    assert!(
        f_body.contains("call const fn user.Shadow.iter"),
        "shadowed source parameter should dispatch to the local class method:\n{output}"
    );
    assert!(
        !f_body.contains("call copy"),
        "shadowed source parameter should not lower through interface dispatch:\n{output}"
    );
}

// ─── Phase 4: type.of concrete types ─────────────────────────────────

/// `type.of<User>()` should lower to `_N = load_type(Concrete(User))`.
#[test]
fn reflect_type_of_class() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
        class User { name string }
        function f() -> type {
            type.of<User>()
        }
        "#,
    );
    mir_snapshot!("reflect_type_of_class", render_mir(&db, file));
}

/// `type.of<int[]>()` — concrete array type.
#[test]
fn reflect_type_of_array() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
        function f() -> type {
            type.of<int[]>()
        }
        "#,
    );
    mir_snapshot!("reflect_type_of_array", render_mir(&db, file));
}

// ─── Phase 5: type.of with generic type params ───────────────────────

/// `type.of<T>()` inside a generic function should lower to
/// `_N = load_type(TypeArgRef(0))`.
#[test]
fn reflect_type_of_bare_typevar() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
        function f<T>() -> type {
            type.of<T>()
        }
        "#,
    );
    mir_snapshot!("reflect_type_of_bare_typevar", render_mir(&db, file));
}

/// `type.of<T[]>()` — composite array wrapping a type-var.
/// Should lower to `_N = load_type(Array(TypeArgRef(0)))`.
#[test]
fn reflect_type_of_array_of_typevar() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
        function f<T>() -> type {
            type.of<T[]>()
        }
        "#,
    );
    mir_snapshot!("reflect_type_of_array_of_typevar", render_mir(&db, file));
}

/// Runtime type syntax is consumed from hir_ty's durable plan: bind the
/// lexical slot once, pass the stored runtime type operand to the generic call,
/// retain its checked-call flag, and use the bound value for `is T`.
#[test]
fn runtime_type_plan_operations_are_explicit() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function accept<T>(value: T) -> T { value }

function f(t: type, value: unknown) -> bool {
    type T = unreflect(t)
    let result = accept<unreflect(t)>(value)
    result is T && result is unreflect(t)
}
"#,
    );
    baml_project::testing::assert_no_diagnostic_errors(&db);
    mir_snapshot!(
        "runtime_type_plan_operations_are_explicit",
        render_mir(&db, file)
    );
}

/// A source-less callable keeps its symbolic package target all the way into
/// MIR while runtime generic operands still come exclusively from the solved
/// call plan.
#[test]
fn mounted_loc_free_runtime_call_target_is_explicit() {
    let mut library = make_db();
    library.add_compiler2_virtual_file(
        "<builtin>/app/lib.baml",
        "function accept<T>(value: T) -> T { value }",
    );
    baml_project::testing::assert_no_diagnostic_errors(&library);
    let interface = baml_compiler2_hir_ty::package_interface::package_interface(
        &library,
        baml_compiler2_hir::package::PackageId::new(&library, baml_base::Name::new("app")),
    );
    let blob = borsh::to_vec(interface).expect("serialize mounted interface");

    let mut db = make_db();
    db.set_mounted_packages([("app".to_string(), blob)].into());
    let file = db.add_file(
        "test.baml",
        r#"
function f(t: type, value: unknown) -> unknown {
    app.accept<unreflect(t)>(value)
}
"#,
    );
    baml_project::testing::assert_no_diagnostic_errors(&db);
    mir_snapshot!(
        "mounted_loc_free_runtime_call_target_is_explicit",
        render_mir(&db, file)
    );
}

/// Bare `$id` read is a special form: it must lower to a call of
/// `baml.id.current` — never to a name lookup or a local read.
#[test]
fn runtime_id_read_lowers_to_baml_id_current() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
        function f() -> string {
            $id
        }
        "#,
    );
    mir_snapshot!(
        "runtime_id_read_lowers_to_baml_id_current",
        render_mir(&db, file)
    );
}

/// `$id = e` is the write special form: it must lower to `baml.id.set(e)` —
/// never to an assignment into a (silently dead) temp.
#[test]
fn runtime_id_assignment_lowers_to_baml_id_set() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
        function f() -> string {
            let next = baml.id.new();
            $id = next;
            $id
        }
        "#,
    );
    mir_snapshot!(
        "runtime_id_assignment_lowers_to_baml_id_set",
        render_mir(&db, file)
    );
}

// ============================================================================
// Array rest-pattern bindings (B-531)
// ============================================================================

/// `[let a, ..let r, let z]` projects the middle as
/// `baml.Array.slice(xs, prefix_len, len - suffix_len)` behind a `len >= 2`
/// guard.
#[test]
fn array_rest_binding_with_suffix_slices_middle() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
        function f(xs: int[]) -> int {
            match (xs) {
                [let a, ..let r, let z] => r.length() + a + z,
                _ => 0
            }
        }
        "#,
    );
    mir_snapshot!(
        "array_rest_binding_with_suffix_slices_middle",
        render_mir(&db, file)
    );
}

/// With no suffix the slice end is the array length directly; no subtraction.
#[test]
fn array_rest_binding_no_suffix_slices_to_len() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
        function f(xs: int[]) -> int {
            match (xs) {
                [let a, ..let r] => r.length() + a,
                _ => 0
            }
        }
        "#,
    );
    mir_snapshot!(
        "array_rest_binding_no_suffix_slices_to_len",
        render_mir(&db, file)
    );
}

/// `.._` binds nothing, so it must lower exactly like bare `..`: no
/// `baml.Array.slice` call, no copied middle. This is a requirement on the
/// ungated implementation, not a snapshot of current behavior.
#[test]
fn array_rest_wildcard_skips_slice_projection() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
        function f(xs: int[]) -> int {
            match (xs) {
                [let a, .._] => a,
                _ => 0
            }
        }
        "#,
    );
    let output = render_mir(&db, file);
    assert!(
        !output.contains("baml.Array.slice"),
        "wildcard rest must not pay for a slice copy:\n{output}"
    );
}
