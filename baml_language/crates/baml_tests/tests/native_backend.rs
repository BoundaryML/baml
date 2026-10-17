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

function for_sum(xs: int[]) -> int {
  let total = 0;
  for (let v in xs) { total += v; }
  total
}

function build(n: int) -> int[] {
  let out: int[] = [];
  let i = 0;
  while (i < n) { out.push(i); i += 1; }
  out
}

function origin() -> Point { Point { x: 0, y: 0 } }

function label(n: int) -> string { n.to_string() }

function with_default(x: int, y: int = 5) -> int { x + y }

function greet(name: string) -> bool { name == "build" }

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
        "user.for_sum",
        "user.build",
        "user.origin",
        "user.label",
        "user.with_default",
        "user.greet",
    ] {
        assert!(
            admitted.contains(&name),
            "{name} should compile; got {admitted:?}"
        );
    }
    // A function that calls bytecode (`main` builds strings through stdlib
    // BAML; `calls_rejected` calls a function left as bytecode) compiles as
    // a resumable body that suspends on those calls.
    let resumable = |name: &str| {
        module
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("{name} should compile"))
            .resumable
    };
    assert!(resumable("user.main"));
    assert!(resumable("user.calls_rejected"));
    assert!(!resumable("user.fib"));
    assert!(!resumable("user.sum"));

    let closure = rejection(&module, "user.uses_closure");
    assert!(
        matches!(closure, Rejection::Unsupported(_)),
        "closures are outside the subset: {closure}"
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
fn generated_module_is_well_formed_rust() {
    let module = compile(FIXTURE);
    syn::parse_file(&module.source).expect("generated module parses as Rust");
    assert!(module.source.contains("pub static NATIVE_BODIES"));
    assert!(module.source.contains("include_bytes!(\"program.bexa\")"));
    // Compacted: whitespace out, and a trailing comma `prettyplease` adds to
    // a wrapped argument list with it, so the formatting of a long entry
    // cannot fail the check.
    let compact: String = module
        .source
        .split_whitespace()
        .collect::<String>()
        .replace(",)", ")");
    for function in &module.functions {
        // Compacted like the source: an impl body's name has spaces in it
        // (`<(ai.Prompt as baml.ToString)>.to_string`).
        let entry: String = format!(
            "CompiledEntry{{object:{},name:{:?},arity:{},body:CompiledFn::{}({}),\
             slots:{},cause_slot:{:?},}}",
            function.object,
            function.name,
            function.arity,
            if function.resumable {
                "Resumable"
            } else {
                "Inline"
            },
            function.symbol,
            function.slots,
            function.cause_slot,
        )
        .split_whitespace()
        .collect();
        assert!(
            compact.contains(&entry),
            "{} is registered under its object index",
            function.name
        );
    }
    // Every compiled call gets a frame (depth limit, traces, telemetry).
    assert!(compact.contains("vm.call_compiled("));
    // An error leaving a compiled function records the line it was raised on.
    assert!(compact.contains("vm.note_line(depth,line)"));
    // A string literal is the program's own string object, not a new one.
    assert!(!compact.contains("vm.string_value("));
    // A defaulted parameter stays a `Value` until MIR's `OmittedArg` test.
    let with_default = body_of(&module, "user.with_default");
    assert!(
        with_default
            .split_whitespace()
            .collect::<String>()
            .contains("letmutl2:Value=args[1];"),
        "{with_default}"
    );
    // Statically typed int arithmetic stays on raw operands.
    assert!(compact.contains("vm.add_i64("));
    // The float loop keeps raw floats and boxes only when storing a Value.
    assert!(compact.contains("vm.float_value("));
}

/// The generated body of one function, for shape assertions.
fn body_of(module: &NativeModule, name: &str) -> String {
    let symbol = &module
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("{name} should compile"))
        .symbol;
    let start = module
        .source
        .find(&format!("pub fn {symbol}("))
        .expect("body is emitted");
    let end = module.source[start..]
        .find("\n}\n")
        .map_or(module.source.len(), |end| start + end);
    module.source[start..end].to_string()
}

/// A resumable body's arguments arrive in slots `0..arity` while local `i`
/// keeps slot `i`: its start reads every argument before it writes any slot,
/// or writing local 1 would overwrite argument 2 before it is read.
#[test]
fn resumable_start_reads_every_argument_before_writing_a_slot() {
    let module = compile(
        r#"
function dur() -> baml.time.Duration { baml.time.Duration.from_milliseconds(1) }
function two(a: int, b: int) -> int { baml.sys.sleep(dur()); a * 10 + b }
"#,
    );
    let body = body_of(&module, "user.two");
    let start = body
        .split("Resume::Start =>")
        .nth(1)
        .and_then(|rest| rest.split("bb =").next())
        .expect("a resumable body starts");
    let last_read = start.rfind("vm.slot(").expect("arguments are read");
    let first_write = start.find("vm.set_slot(").unwrap_or(usize::MAX);
    assert!(last_read < first_write, "{start}");
}

/// A namespace `Foo` with a function `m` and a class `Foo` with a method `m`
/// render alike (`user.Foo.m`). The backend binds a function by its identity
/// in the program, so each compiles against its own object, and the name the
/// loader checks each entry against is that object's own.
#[test]
fn functions_whose_names_render_alike_compile_to_their_own_objects() {
    let db = baml_test_support::setup_multi_file_db(&[
        (
            "foo.baml",
            "class Foo {\n  x int\n  function m(self) -> int { self.x + 1 }\n}\n",
        ),
        ("ns_Foo/m.baml", "function m(x: int) -> int { x + 2 }\n"),
    ]);
    let root = db.workspace_root().expect("test db has a workspace root");
    let program =
        baml_db::compile_program(&db, root, OptLevel::Two).expect("fixture compiles to bytecode");
    let module = compile_program(&db, root, &program, OptLevel::Two, "program.bexa");

    let alike: Vec<_> = module
        .functions
        .iter()
        .filter(|f| f.name == "user.Foo.m")
        .collect();
    assert_eq!(
        alike.len(),
        2,
        "both compile under one rendered name: {:?}",
        module
            .functions
            .iter()
            .map(|f| &f.name)
            .chain(module.rejected.iter().map(|(name, _)| name))
            .collect::<Vec<_>>()
    );
    assert_ne!(alike[0].object, alike[1].object, "each has its own object");
    assert_ne!(alike[0].symbol, alike[1].symbol, "each has its own symbol");
    for function in &alike {
        assert_eq!(
            program
                .function_name(bex_vm_types::ObjectIndex::from_raw(function.object))
                .as_deref(),
            Some(function.name.as_str()),
            "the entry's name is what the loader renders for its object"
        );
    }
}
