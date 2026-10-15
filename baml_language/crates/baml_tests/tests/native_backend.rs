//! The BAML-to-Rust backend admits the compute-shaped subset and keeps the
//! rest bytecode, with a reason, and its output is well-formed Rust.
//!
//! Rust rather than a BAML test because the backend's output is Rust source
//! and its admission table is compiler-side data a program cannot observe.

use baml_compiler2_emit::OptLevel;
use baml_db::native::{NativeModule, Rejection, compile_program};

fn compile(source: &str) -> NativeModule {
    let db = baml_test_support::setup_test_db(source);
    let root = db.workspace_root().expect("test db has a workspace root");
    let program =
        baml_db::compile_program(&db, root, OptLevel::Two).expect("fixture compiles to bytecode");
    compile_program(&db, root, &program, OptLevel::Two, "program.bexa")
}

const FIXTURE: &str = r#"
class Point {
  x int
  y int
}

function sum(xs: int[]) -> int {
  let total = 0;
  let i = 0;
  while (i < xs.length()) {
    total = total + xs[i];
    i = i + 1;
  }
  total
}

function fib(n: int) -> int {
  if (n < 2) { n } else { fib(n - 1) + fib(n - 2) }
}

function manhattan(p: Point, q: Point) -> int {
  let dx = p.x - q.x;
  let dy = p.y - q.y;
  (if (dx < 0) { 0 - dx } else { dx }) + (if (dy < 0) { 0 - dy } else { dy })
}

function safe_div(a: int, b: int) -> int {
  (a / b) catch (e) { baml.panics.DivisionByZero => -1 }
}

function mean(xs: float[]) -> float {
  let total = 0.0;
  let i = 0;
  while (i < xs.length()) {
    total = total + xs[i];
    i = i + 1;
  }
  total / 2.0
}

function uses_closure(xs: int[]) -> int[] {
  xs.map((x: int) => { x * 2 })
}

function calls_rejected(xs: int[]) -> int {
  uses_closure(xs).length()
}

function generic<T>(x: T) -> T { x }

function main() -> string {
  let p = Point { x: 1, y: 2 };
  let q = Point { x: 4, y: 6 };
  `${sum([1, 2, 3])}|${fib(10)}|${manhattan(p, q)}|${safe_div(7, 0)}|${mean([1.0, 3.0])}`
}
"#;

fn rejection<'a>(module: &'a NativeModule, name: &str) -> &'a Rejection {
    &module
        .rejected
        .iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("{name} should be rejected"))
        .1
}

#[test]
fn compute_shaped_functions_are_admitted_and_the_rest_stay_bytecode() {
    let module = compile(FIXTURE);
    let admitted: Vec<&str> = module.functions.iter().map(|f| f.name.as_str()).collect();
    for name in [
        "user.sum",
        "user.fib",
        "user.manhattan",
        "user.safe_div",
        "user.mean",
    ] {
        assert!(
            admitted.contains(&name),
            "{name} should compile; got {admitted:?}"
        );
    }
    // `main` builds strings through stdlib bytecode it cannot call directly.
    assert!(!admitted.contains(&"user.main"));

    let closure = rejection(&module, "user.uses_closure");
    assert!(
        matches!(closure, Rejection::Unsupported(_)),
        "closures are outside the subset: {closure}"
    );
    let caller = rejection(&module, "user.calls_rejected");
    assert!(
        matches!(caller, Rejection::Unsupported(reason) if reason.contains("uses_closure")),
        "a caller of a rejected function is rejected for that reason: {caller}"
    );
    assert!(matches!(
        rejection(&module, "user.generic"),
        Rejection::Unsupported(reason) if reason.contains("generic")
    ));
    assert!(
        module
            .rejected
            .iter()
            .all(|(_, rejection)| matches!(rejection, Rejection::Unsupported(_))),
        "a checked program never produces invalid MIR: {:?}",
        module.rejected
    );
}

#[test]
fn loops_make_a_function_observed() {
    let module = compile(FIXTURE);
    let observed = |name: &str| {
        module
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("{name} should compile"))
            .observed
    };
    assert!(observed("user.sum"));
    assert!(observed("user.mean"));
    assert!(!observed("user.fib"));
    assert!(!observed("user.manhattan"));
}

#[test]
fn generated_module_is_well_formed_rust() {
    let module = compile(FIXTURE);
    syn::parse_file(&module.source).expect("generated module parses as Rust");
    assert!(module.source.contains("pub static NATIVE_BODIES"));
    assert!(module.source.contains("include_bytes!(\"program.bexa\")"));
    for function in &module.functions {
        assert!(
            module
                .source
                .contains(&format!("({:?}, {})", function.name, function.symbol)),
            "{} is registered under its name",
            function.name
        );
    }
    // Statically typed int arithmetic stays on raw operands.
    assert!(module.source.contains("vm.add_i64("));
    // The float loop keeps raw floats and boxes only when storing a Value.
    assert!(module.source.contains("vm.float_value("));
}
