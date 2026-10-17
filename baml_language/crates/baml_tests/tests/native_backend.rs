//! The BAML-to-Rust backend admits the compute-shaped subset and keeps the
//! rest bytecode, with a reason, and its output is well-formed Rust.
//!
//! Rust rather than a BAML test because the backend's output is Rust source
//! and its admission table is compiler-side data a program cannot observe.

use baml_compiler2_emit::OptLevel;
use baml_db::native::{NativeModule, Rejection, compile_program};
use bex_vm_types::{Object, ObjectIndex, RealizedTy, types::Program};

fn compile(source: &str) -> NativeModule {
    compile_with_program(source).0
}

fn compile_with_program(source: &str) -> (NativeModule, Program) {
    let db = baml_test_support::setup_test_db(source);
    let root = db.workspace_root().expect("test db has a workspace root");
    let program =
        baml_db::compile_program(&db, root, OptLevel::Two).expect("fixture compiles to bytecode");
    let module = compile_program(&db, root, &program, OptLevel::Two, "program.bexa", None);
    (module, program)
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
    // A generic function compiles too: one body for every instantiation.
    assert!(admitted.contains(&"user.generic"), "{admitted:?}");
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

const INTERFACES: &str = r#"
interface Shape {
  function area(self) -> int throws never
  function scaled(self, by: int) -> int throws never { self.area() * by }
}

class Square {
  side int
  implements Shape {
    function area(self) -> int throws never { self.side * self.side }
  }
}

class Rect {
  w int
  h int
  implements Shape {
    function area(self) -> int throws never { self.w * self.h }
  }
}

interface Named {
  name: string
}

class Person {
  name: string
  implements Named {}
}

function total(shapes: Shape[]) -> int {
  let sum = 0;
  for (let s in shapes) { sum += s.area(); }
  sum
}

function scaled(s: Shape, by: int) -> int { s.scaled(by) }

function area_fn(s: Shape) -> () -> int throws never { s.area }

function rename(n: Named, to: string) -> string {
  n.name = to;
  n.name
}

function describe(x: int) -> string { x.to_string() }
"#;

/// An interface method call on a receiver whose implementation is not known
/// at compile time resolves it at run time, through the VM, so the function
/// compiles as a resumable body that requests the call; interface fields and
/// method values resolve inline.
#[test]
fn interface_dispatch_compiles_as_a_runtime_resolved_call() {
    let module = compile(INTERFACES);
    for name in [
        "user.total",
        "user.scaled",
        "user.area_fn",
        "user.rename",
        "user.describe",
    ] {
        let function = module
            .functions
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("{name} should compile; rejected: {:?}", module.rejected));
        // Only the calls suspend; a method value, a field and a call on a
        // primitive (devirtualized by MIR) resolve in place.
        assert_eq!(
            function.resumable,
            name == "user.total" || name == "user.scaled",
            "{name}"
        );
    }
    // No dispatch keeps a function of the fixture bytecode. (The stdlib
    // compiled beside it has interfaces instantiated with frame type
    // arguments and functions typed as interfaces, which stay bytecode.)
    assert!(
        module
            .rejected
            .iter()
            .filter(|(name, _)| name.starts_with("user."))
            .all(|(_, rejection)| !rejection.to_string().contains("interface")),
        "{:?}",
        module.rejected
    );
    let compact = |name: &str| {
        body_of(&module, name)
            .split_whitespace()
            .collect::<String>()
    };
    // The call site owns its inline cache, resolves through it, and lays the
    // arguments over the callee it resolved before requesting the call.
    let total = compact("user.total");
    assert!(total.contains("staticS"), "{total}");
    assert!(total.contains("vm.virtual_target(&S"), "{total}");
    assert!(
        total.contains("ifletSome(v)=vm.call_inline(callee,&["),
        "{total}"
    );
    assert!(
        total.contains("vm.push_virtual_args(callee,None,&["),
        "{total}"
    );
    assert!(total.contains("Step::Call{callee,argc}"), "{total}");
    let rename = compact("user.rename");
    assert!(rename.contains("vm.virtual_store_field("), "{rename}");
    assert!(rename.contains("vm.virtual_load_field("), "{rename}");
    let area_fn = compact("user.area_fn");
    assert!(
        area_fn.contains("vm.make_virtual_bound_method("),
        "{area_fn}"
    );
    syn::parse_file(&module.source).expect("generated module parses as Rust");
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
    let module = compile_program(&db, root, &program, OptLevel::Two, "program.bexa", None);

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

/// Generic functions compile polymorphically: one body per function that
/// reads `T` off its frame (`vm.frame_type_arg`) and builds every type it
/// derives from it once on entry, for its `is T` tests, `T[]` arrays,
/// generic class instances, nested generic calls, and generic function
/// values; a fully concrete `foo<int>` is the program's pooled object.
#[test]
fn generic_functions_compile_against_the_frame() {
    const SOURCE: &str = r#"
class Point { x int y int }
class Box<T> {
  value T
  function get(self) -> T { self.value }
}
function identity<T>(x: T) -> T { x }
function is_t<T>(witness: T, x: int | string) -> bool { x is T }
function singleton<T>(x: T) -> T[] { [x] }
function twice<T>(x: T) -> T { identity<T>(identity<T>(x)) }
function boxed<T>(x: T) -> Box<T> { Box { value: x } }
function unbox<T>(b: Box<T>) -> T { b.get() }
function is_box_int(x: Box<int> | Box<string>) -> bool { x is Box<int> }
function id_int() -> (int) -> int throws never { identity<int> }
function id_point() -> (Point) -> Point throws never { identity<Point> }
function id_of<T>() -> (T) -> T throws never { identity<T> }
function is_int_t<T>() -> bool { reflect.Type.of<T>() == reflect.Type.of<int>() }
function bound(t: reflect.Type) -> bool { type U = unreflect(t); reflect.Type.of<U>() == t }
"#;
    let (module, program) = compile_with_program(SOURCE);
    let admitted: Vec<&str> = module.functions.iter().map(|f| f.name.as_str()).collect();
    for name in [
        "user.identity",
        "user.is_t",
        "user.singleton",
        "user.twice",
        "user.boxed",
        "user.unbox",
        "user.is_box_int",
        "user.id_int",
        "user.id_point",
        "user.id_of",
        "user.is_int_t",
        "user.bound",
        "user.Box.get",
    ] {
        assert!(
            admitted.contains(&name),
            "{name} should compile; rejected: {:?}",
            module.rejected
        );
    }
    let compact = |name: &str| {
        body_of(&module, name)
            .split_whitespace()
            .collect::<String>()
    };
    // The frame's type argument, read once on entry; `is T` is membership in it.
    let is_t = compact("user.is_t");
    assert!(
        is_t.contains("letty0:RealizedTy=vm.frame_type_arg(depth,0)?;"),
        "{is_t}"
    );
    assert!(is_t.contains("vm.value_is_member_of("), "{is_t}");
    // `T[]` is an array of the frame's type.
    let singleton = compact("user.singleton");
    assert!(
        singleton.contains("vm.new_array(ty0.clone(),"),
        "{singleton}"
    );
    // A nested generic call passes the substituted type on.
    let twice = compact("user.twice");
    assert!(twice.contains("vm.call_compiled_with_types("), "{twice}");
    assert!(twice.contains("&[ty0.clone()]"), "{twice}");
    // A generic class instance carries its class type arguments ...
    let boxed = compact("user.boxed");
    assert!(boxed.contains("vm.new_generic_instance("), "{boxed}");
    // ... and a call of its method passes them as the owner's arguments.
    let unbox = compact("user.unbox");
    assert!(unbox.contains("vm.call_compiled_with_types("), "{unbox}");
    // `is Box<int>` compares the instance's arguments.
    let is_box_int = compact("user.is_box_int");
    assert!(is_box_int.contains("vm.is_instance_of_at("), "{is_box_int}");
    assert!(is_box_int.contains("&[RealizedTy::Int]"), "{is_box_int}");
    // `foo<int>` and `foo<Point>` are the program's pooled objects, so
    // identity is the program's; `foo<T>` is built from the frame.
    let is_int = |ty: &RealizedTy| matches!(ty, RealizedTy::Int);
    let is_class = |ty: &RealizedTy| matches!(ty, RealizedTy::Class(..));
    for (name, is_type_arg) in [
        ("user.id_int", &is_int as &dyn Fn(&RealizedTy) -> bool),
        ("user.id_point", &is_class),
    ] {
        let body = compact(name);
        let pooled = body
            .split("Value::object(vm.idx_to_ptr(ObjectIndex::from_raw(")
            .nth(1)
            .and_then(|rest| rest.split(')').next())
            .and_then(|index| index.parse::<usize>().ok())
            .unwrap_or_else(|| panic!("{name} loads a pooled object: {body}"));
        let Object::GenericFunction(generic) = &program.objects[ObjectIndex::from_raw(pooled)]
        else {
            panic!("{name} loads object {pooled}, not a generic function")
        };
        assert_eq!(generic.type_args.len(), 1);
        assert!(
            is_type_arg(&generic.type_args[0]),
            "{name}: {:?}",
            generic.type_args
        );
    }
    let id_of = compact("user.id_of");
    assert!(
        id_of.contains("vm.make_generic_function(depth,GlobalIndex::from_raw("),
        "{id_of}"
    );
    // A body binding a type (`type U = t`) builds its derived types where
    // they are used, after the binding, not on entry.
    let bound = compact("user.bound");
    assert!(bound.contains("vm.bind_type(depth,"), "{bound}");
    assert!(!bound.contains("letty0:"), "{bound}");
    // Reflecting `T` passes the frame's type to the builtin.
    let is_int_t = compact("user.is_int_t");
    assert!(is_int_t.contains("ty0.clone()"), "{is_int_t}");
    syn::parse_file(&module.source).expect("generated module parses as Rust");
}

/// With entry points, only what they reach compiles: the rest is reported
/// as unreached, not as unsupported, and nothing an entry calls is lost
/// (`main` reaches the stdlib's string building; `fib` reaches itself).
#[test]
fn entry_points_prune_admission_to_what_they_reach() {
    let db = baml_test_support::setup_test_db(FIXTURE);
    let root = db.workspace_root().expect("test db has a workspace root");
    let program =
        baml_db::compile_program(&db, root, OptLevel::Two).expect("fixture compiles to bytecode");
    let callables = program.rendered_callables();
    let entries = [callables["user.fib"].object.raw()];
    let module = compile_program(
        &db,
        root,
        &program,
        OptLevel::Two,
        "program.bexa",
        Some(&entries),
    );
    let admitted: Vec<&str> = module.functions.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(admitted, ["user.fib"], "only the entry's closure compiles");
    assert!(matches!(
        rejection(&module, "user.sum"),
        Rejection::Unreached
    ));
    assert!(matches!(
        rejection(&module, "user.main"),
        Rejection::Unreached
    ));
    assert!(
        module
            .rejected
            .iter()
            .any(|(name, rejection)| !name.starts_with("user.")
                && matches!(rejection, Rejection::Unreached)),
        "the stdlib is pruned too"
    );
    // Closures stay bytecode, but what they reach is still reached.
    assert!(matches!(
        rejection(&module, "user.uses_closure"),
        Rejection::Unreached
    ));

    let entries = [callables["user.main"].object.raw()];
    let module = compile_program(
        &db,
        root,
        &program,
        OptLevel::Two,
        "program.bexa",
        Some(&entries),
    );
    let admitted: Vec<&str> = module.functions.iter().map(|f| f.name.as_str()).collect();
    for name in [
        "user.main",
        "user.sum",
        "user.fib",
        "user.manhattan",
        "user.safe_div",
        "user.mean",
    ] {
        assert!(
            admitted.contains(&name),
            "{name} is reached from main: {admitted:?}"
        );
    }
    assert!(
        admitted.iter().any(|name| name.starts_with("baml.")),
        "main's string building reaches the stdlib: {admitted:?}"
    );
    assert!(matches!(
        rejection(&module, "user.build"),
        Rejection::Unreached
    ));
}
