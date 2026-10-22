//! The BAML-to-Rust backend admits the compute-shaped subset and keeps the
//! rest bytecode, with a reason, and its output is well-formed Rust.
//!
//! Rust rather than a BAML test because the backend's output is Rust source
//! and its admission table is compiler-side data a program cannot observe.

use baml_compiler2_emit::OptLevel;
use baml_db::rust_backend::{Rejection, RustModule, compile_program};
use bex_vm_types::{Instruction, Object, ObjectIndex, types::Program};

fn fingerprint(program: &Program) -> [u8; 32] {
    baml_artifact::encode_with_digest(baml_artifact::ArtifactKind::Program, program)
        .unwrap()
        .1
}

fn compile(source: &str) -> RustModule {
    compile_with_program(source).0
}

fn compile_with_program(source: &str) -> (RustModule, Program) {
    let db = baml_test_support::setup_test_db(source);
    let root = db.workspace_root().expect("test db has a workspace root");
    let program =
        baml_db::compile_program(&db, root, OptLevel::Two).expect("fixture compiles to bytecode");
    let module = compile_program(
        &db,
        root,
        &program,
        OptLevel::Two,
        "program.bexa",
        fingerprint(&program),
        None,
    )
    .expect("generated module renders");
    (module, program)
}

/// Cached/artifact programs lack the compile-time MIR-to-PC map. The
/// existing MIR shape guard must refuse to generate compiled bodies for them.
#[test]
fn deserialized_program_cannot_supply_compiled_bodies() {
    let db = baml_test_support::setup_test_db("function answer() -> int { 42 }");
    let root = db.workspace_root().expect("test db has a workspace root");
    let program =
        baml_db::compile_program(&db, root, OptLevel::Two).expect("fixture compiles to bytecode");
    let fresh = compile_program(
        &db,
        root,
        &program,
        OptLevel::Two,
        "program.bexa",
        fingerprint(&program),
        None,
    )
    .expect("generated module renders");
    assert!(fresh.functions.iter().any(|f| f.name == "user.answer"));

    let serialized = borsh::to_vec(&program).unwrap();
    let restored: Program = borsh::from_slice(&serialized).unwrap();
    let cached = compile_program(
        &db,
        root,
        &restored,
        OptLevel::Two,
        "program.bexa",
        fingerprint(&restored),
        None,
    )
    .expect("generated module renders");
    assert!(cached.functions.is_empty());
    assert!(matches!(
        rejection(&cached, "user.answer"),
        Rejection::Unsupported(reason) if reason.contains("without MIR positions")
    ));
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

fn rejection<'a>(module: &'a RustModule, name: &str) -> &'a Rejection {
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
    for name in ["user.main", "user.calls_rejected", "user.uses_closure"] {
        assert!(
            admitted.contains(&name),
            "{name} should compile; got {admitted:?}"
        );
    }
    let uses_closure = body_of(&module, "user.uses_closure");
    assert!(uses_closure.contains("vm.suspend("), "{uses_closure}");
    let body = body_of(&module, "user.<lambda(uses_closure, 0)>");
    assert!(!body.contains("vm.suspend("), "{body}");
    assert!(!body.contains("vm.should_yield()"), "{body}");
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
    let (module, program) = compile_with_program(FIXTURE);
    syn::parse_file(&module.source).expect("generated module parses as Rust");
    assert!(module.source.contains("pub static COMPILED_BODIES"));
    assert!(module.source.contains("include_bytes!(\"program.bexa\")"));
    let compact: String = module.source.split_whitespace().collect();
    let fingerprint = format!("fingerprint:{:?}", fingerprint(&program))
        .split_whitespace()
        .collect::<String>();
    assert_eq!(compact.matches(&fingerprint).count(), 1);
    for function in &module.functions {
        // Compacted like the source: an impl body's name has spaces in it
        // (`<(ai.Prompt as baml.ToString)>.to_string`).
        let entry: String = format!(
            "CompiledEntry{{object:{},name:{:?},arity:{},body:{},\
             slots:{},cause_slot:{:?}}}",
            function.object,
            function.name,
            function.arity,
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
}

/// A compiled body records real bytecode PCs: each call passes the index of
/// its call instruction in the function's own bytecode, so the caller's frame
/// reads as the bytecode frame would there (the line of a trace, the shield
/// table, the call site telemetry records).
#[test]
fn a_compiled_call_passes_its_call_instruction() {
    let (module, program) = compile_with_program(FIXTURE);
    let fib = module
        .functions
        .iter()
        .find(|function| function.display == "user.fib")
        .expect("fib compiles");
    let Object::Function(function) = &program.objects[ObjectIndex::from_raw(fib.object)] else {
        panic!("fib is a function");
    };
    let calls: Vec<usize> = function
        .bytecode
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, instruction)| matches!(instruction, Instruction::Call { .. }))
        .map(|(pc, _)| pc)
        .collect();
    assert_eq!(calls.len(), 2, "{:?}", function.bytecode.instructions);
    // The last argument of each `vm.call_compiled(..)`.
    let body: String = body_of(&module, "user.fib").split_whitespace().collect();
    let mut passed: Vec<usize> = Vec::new();
    for (start, _) in body.match_indices("vm.call_compiled(") {
        let args = &body[start + "vm.call_compiled(".len()..];
        let mut depth = 0;
        let end = args
            .char_indices()
            .find(|&(_, ch)| {
                match ch {
                    '(' | '[' => depth += 1,
                    ')' | ']' => depth -= 1,
                    _ => {}
                }
                depth < 0
            })
            .map(|(end, _)| end)
            .expect("the call is closed");
        let last = args[..end]
            .trim_end_matches(',')
            .rsplit(',')
            .next()
            .expect("an argument");
        let pc = last
            .parse::<usize>()
            .unwrap_or_else(|_| panic!("a literal PC: {last}"));
        passed.push(pc);
    }
    passed.sort_unstable();
    assert_eq!(passed, calls, "{body}");
}

/// The generated body of one function, for shape assertions.
fn body_of(module: &RustModule, name: &str) -> String {
    let symbol = &module
        .functions
        .iter()
        .find(|f| f.display == name)
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

fn assert_admitted(module: &RustModule, names: &[&str]) {
    let admitted: Vec<&str> = module
        .functions
        .iter()
        .map(|f| f.display.as_str())
        .collect();
    for name in names {
        assert!(
            admitted.contains(name),
            "{name} should compile; rejected: {:?}",
            module.rejected
        );
    }
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

const CLOSURES: &str = r#"
function cap_local(n: int) -> int {
  let total = 0;
  let add = (x: int) -> void { total = total + x; };
  add(n);
  add(n);
  total
}

function cap_param(n: int) -> int {
  let bump = () -> int { n = n + 1; n };
  bump();
  bump()
}

function loop_cells() -> int {
  let total = 0;
  for (let i = 0; i < 3; i += 1) {
    let f = () -> int { i };
    total = total + f();
  }
  total
}

function nested(a: int) -> int {
  let outer = (b: int) -> int {
    let inner = (c: int) -> int { a + b + c };
    inner(1)
  };
  outer(2)
}

function stored(xs: int[]) -> int[] {
  let double = (x: int) -> int { x * 2 };
  xs.map(double)
}

function main() -> string {
  `${cap_local(2)}|${cap_param(1)}|${loop_cells()}|${nested(3)}|${stored([1, 2])}`
}
"#;

const RUNTIME_TYPES: &str = r#"
interface Conv<T> {
    function describe(self) -> string throws never
}

class Both {
    implements Conv<int> {
        function describe(self) -> string throws never { "as int" }
    }
}

class Square {
    side int
    function scaled(self, k: int) -> int { self.side * k }
}

type Tree = int | Tree[]

function generic_describe<T>(c: Conv<T>) -> string throws never { c.describe() }

function fns() -> int {
    let fs: ((int) -> int throws never)[] = [];
    fs.push((x: int) -> int { x + 1 });
    fs.length()
}

function is_tree(x: int | string | Tree[]) -> bool { x is Tree }

function bound() -> int {
    let sq = Square { side: 5 };
    let f = sq.scaled;
    f(3)
}
"#;

const LET_TYPE_BOUND: &str = r#"
interface Named {
    name: string,
}

class Person {
    name: string,

    implements Named {}
}

function bind_named(t: reflect.Type) -> string {
    let type T extends Named = t else {
        return "missed";
    };
    "bound"
}

function main() -> string {
    bind_named(Person.type())
}
"#;

const RUNTIME_BOUND_ARGUMENTS: &str = r#"
interface Stamper {
    function stamp<U>(self, u: U) -> string throws never
    function emboss<U>(u: U) -> string throws never
}

class Press {
    implements Stamper {
        function stamp<U>(self, u: U) -> string throws never {
            "stamp " + reflect.Type.of<U>().to_string()
        }
        function emboss<U>(u: U) -> string throws never {
            "emboss " + reflect.Type.of<U>().to_string()
        }
    }
}

function id<T>(x: T) -> T { x }

function bound_call(t: reflect.Type, v: unknown) -> unknown {
    let type T = t;
    match (v) {
        let x: T => id<T>(x),
        _ => null,
    }
}

function bound_dispatch(t: reflect.Type, v: unknown) -> string {
    let p = Press {};
    let type U = t;
    match (v) {
        let x: U => Stamper.stamp<U>(p, x) + (Press as Stamper).emboss<U>(x),
        _ => "none",
    }
}
"#;

const INTERFACE_ARGUMENTS: &str = r#"
interface Speaker {
    function speak(self) -> string throws never
}

class Tagged<T> {
    name: string,
}

function tagged_speaker(text: string) -> string throws baml.errors.ParseError {
    let parsed: unknown = baml.sap.parse<Tagged<Speaker>>(text);
    match (parsed) {
        let tagged: Tagged<Speaker> => tagged.name,
        _ => "other",
    }
}
"#;

const GC_CHECKS: &str = r#"
class Point {
  x: int
  y: int
}

function sum_to(n: int) -> int {
  let total = 0;
  let i = 0;
  while (i < n) { total += i; i += 1; }
  total
}

function build(n: int) -> Point[] {
  let out: Point[] = [];
  let i = 0;
  while (i < n) { out.push(Point { x: i, y: i }); i += 1; }
  out
}

function keep(p: Point, n: int) -> int {
  let points = build(n);
  p.x + points.length()
}

function main() -> int {
  sum_to(10) + keep(Point { x: 1, y: 2 }, 3)
}
"#;

const GENERICS: &str = r#"
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

/// Each construct the backend lowers compiles; the differential probes check
/// what the compiled code does.
#[test]
fn constructs_compile() {
    let cases: &[(&str, &[&str])] = &[
        (
            r#"
enum Color {
  Red
  Green
}

class Circle { r: int }
class Square { s: int }
class Tri { a: int }
class Hex { h: int }

client Probe = openai.ResponsesClient.new(model = "m", api_key = "k", base_url = "http://localhost:1/v1");

function words(xs: string[]) -> map<string, int> {
  let counts: map<string, int> = {};
  for (let x in xs) { counts[x] = (counts.get(x) ?? 0) + 1; }
  counts
}

function squares(n: int) -> map<int, int> {
  let m: map<int, int> = {};
  let i = 0;
  while (i < n) { m[i] = i * i; i += 1; }
  m
}

function keyed(m: map<Circle, int>, c: Circle) -> int { m[c] }

function green() -> Color { Color.Green }

function big(n: bigint) -> bigint { n * 123456789012345678901234567890n }

function bytes() -> int {
  let b = b"\x01\x02";
  b[0] = 9;
  b[0]
}

function model() -> string { Probe.model }

function kind(x: int[] | string[] | map<string, int> | int) -> string {
  if (x is int[]) { "ints" } else if (x is map<string, int>) { "map" } else if (x is int) { "int" } else { "strings" }
}

function area(s: Circle | Square | Tri | Hex) -> int {
  match (s) {
    Circle => s.r,
    Square => s.s,
    Tri => s.a,
    Hex => s.h,
  }
}

function here() -> reflect.Package { reflect.Package.current() }
"#,
            &[
                "user.words",
                "user.squares",
                "user.keyed",
                "user.green",
                "user.big",
                "user.bytes",
                "user.model",
                "user.kind",
                "user.area",
                "user.here",
            ],
        ),
        (
            INTERFACES,
            &[
                "user.total",
                "user.scaled",
                "user.area_fn",
                "user.rename",
                "user.describe",
            ],
        ),
        (
            r#"
interface Grows {
  function grow(self, by: int) -> int throws never
}

class Plot {
  side int
  implements Grows {
    function grow(self, by: int, extra: int = 1) -> int throws never { self.side * by + extra }
  }
}

function grow_by(g: Grows, by: int) -> int { g.grow(by) }
"#,
            &["user.grow_by"],
        ),
        (
            r#"
function tr_and(a: string?, b: int) -> bool {
  a && b
}
"#,
            &["user.tr_and"],
        ),
        (
            r#"
function own_id() -> trace.SpanId? {
  trace.current_span_id()
}

function reservation_reuse_fails() -> bool {
  let reserved = trace.span().reserve();
  let alias = reserved;
  own_id($trace = reserved);
  {
    own_id($trace = alias);
    false
  } catch_all_panics (error) {
    _ => true,
  }
}
"#,
            &["user.reservation_reuse_fails"],
        ),
        (
            r#"
client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

function Describe(topic: string) -> string {
    client: TestClient
    prompt: `Describe ${topic}.`
}

function has_spec(topic: string) -> bool {
  let spec = Describe@spec(topic);
  true
}
"#,
            &["user.has_spec"],
        ),
        (
            r#"
/// baml:$trace=trace.hidden
function hidden(n: int) -> int { n + 1 }

/// baml:$trace=trace.timing
function timing(n: int) -> int { n + 2 }

/// baml:$trace=trace.span
function spanned(n: int) -> int { n + 3 }

/// baml:$trace=trace.empty_span
function empty(n: int) -> int { n + 4 }

function decide(n: int, settings: trace.Settings) -> trace.Options? throws never {
  trace.hidden()
}

/// baml:$trace=decide
function hooked(n: int) -> int { n + 5 }

function plain_hook() -> trace.Options throws never { trace.hidden() }

/// baml:$trace=plain_hook
function hooked_without_settings(n: int) -> int { n + 6 }

function total(words: string[]) -> int {
  let total = 0;
  let i = 0;
  while (i < words.length()) {
    total += words[i].length();
    i += 1;
  }
  for (let k in baml.iter.Range.new(0, 3)) { total += k; }
  total
}
"#,
            &[
                "user.hidden",
                "user.timing",
                "user.spanned",
                "user.empty",
                "user.hooked",
                "user.hooked_without_settings",
                "user.total",
                // Hidden stdlib functions compile, so callers enter them directly.
                "baml.String.length",
                "baml.iter.Range.new",
                "baml.<(baml.iter.Range as baml.iter.Iterator)>.next",
                "baml.<(baml.iter.Range as baml.iter.Iterable)>.iter",
            ],
        ),
        (
            r#"
function iter_of(xs: int[]) -> baml.iter.Iterator<Item = int, Error = never> {
  xs.iter()
}

function first(xs: int[]) -> int {
  let total = 0;
  for (let v in iter_of(xs)) { total += v; }
  total
}
"#,
            &["user.iter_of", "user.first"],
        ),
        (
            r#"
function sum_of(it: baml.iter.Iterator<Item = int, Error = never>) -> int {
  let total = 0;
  for (let v in it) { total += v; }
  total
}

function passed(xs: int[]) -> int {
  sum_of(xs.iter())
}

function chained() -> int[] {
  [1, 2].iter().chain([3, 4]).collect()
}

function lazy(xs: int[]) -> int {
  let it = xs.iter();
  xs.push(10);
  sum_of(it)
}

function mixed(xs: int[]) -> int {
  let total = 0;
  for (let v in xs) { total += v; }
  total + sum_of(xs.iter())
}
"#,
            &["user.passed", "user.chained", "user.lazy", "user.mixed"],
        ),
        (
            GENERICS,
            &[
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
            ],
        ),
        (
            CLOSURES,
            &[
                "user.cap_local",
                "user.<lambda(cap_local, 0)>",
                "user.cap_param",
                "user.<lambda(cap_param, 0)>",
                "user.loop_cells",
                "user.<lambda(loop_cells, 0)>",
                "user.nested",
                "user.<lambda(nested, 0)>",
                "user.<lambda(<lambda(nested, 0)>, 1)>",
                "user.stored",
                "user.<lambda(stored, 0)>",
            ],
        ),
        (
            RUNTIME_TYPES,
            &[
                "user.generic_describe",
                "user.fns",
                "user.is_tree",
                "user.bound",
            ],
        ),
        (LET_TYPE_BOUND, &["user.bind_named"]),
        (
            RUNTIME_BOUND_ARGUMENTS,
            &["user.bound_call", "user.bound_dispatch"],
        ),
        (INTERFACE_ARGUMENTS, &["user.tagged_speaker"]),
        (
            GC_CHECKS,
            &["user.sum_to", "user.build", "user.keep", "user.main"],
        ),
    ];
    for (source, names) in cases {
        let module = compile(source);
        assert_admitted(&module, names);
        syn::parse_file(&module.source).expect("generated module parses as Rust");
    }
}

/// No dispatch keeps a function of the fixture bytecode. (The stdlib
/// compiled beside it has interfaces instantiated with frame type arguments
/// and functions typed as interfaces, which stay bytecode.)
#[test]
fn interface_dispatch_is_admitted() {
    let module = compile(INTERFACES);
    assert!(
        module
            .rejected
            .iter()
            .filter(|(name, _)| name.starts_with("user."))
            .all(|(_, rejection)| !rejection.to_string().contains("interface")),
        "{:?}",
        module.rejected
    );
}

/// A lambda's entry names its own object, as the loader renders it, with
/// the package it belongs to in the report.
#[test]
fn a_lambda_is_a_unit_of_its_own() {
    let module = compile(CLOSURES);
    let lambda = module
        .functions
        .iter()
        .find(|f| f.display == "user.<lambda(cap_local, 0)>")
        .expect("compiles");
    assert_eq!(lambda.name, ".<lambda(cap_local, 0)>");
    assert_eq!(lambda.arity, 1);
    assert!(lambda.symbol.starts_with("user__lambda_cap_local__0__"));
}

/// A loop that alone consumes an array's iterator walks the array by index:
/// no `ArrayIterator` is built and no virtual call is dispatched.
#[test]
fn a_local_loop_over_an_array_is_an_index_walk() {
    let module = compile(
        r#"
function total(xs: int[]) -> int {
  let total = 0;
  for (let v in xs) { total += v; }
  total
}
"#,
    );
    let body = body_of(&module, "user.total")
        .split_whitespace()
        .collect::<String>();
    assert!(body.contains("vm.load_array_element("), "{body}");
    assert!(body.contains("vm.container_len("), "{body}");
    assert!(!body.contains("vm.virtual_target("), "{body}");
    assert!(!body.contains("ArrayIterator"), "{body}");
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
    let module = compile_program(
        &db,
        root,
        &program,
        OptLevel::Two,
        "program.bexa",
        fingerprint(&program),
        None,
    )
    .expect("generated module renders");

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
        fingerprint(&program),
        Some(&entries),
    )
    .expect("generated module renders");
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
    // A closure in a function no entry reaches is unreached with its maker.
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
        fingerprint(&program),
        Some(&entries),
    )
    .expect("generated module renders");
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

const SIBLING_BINDINGS: &str = r#"
class Wrapper<T> { inner T }

function sibling_bindings(a: reflect.Type, b: reflect.Type) -> reflect.Type[] {
    let first: reflect.Type = {
        let type T = a;
        reflect.Type.of<Wrapper<T>>()
    }
    let second: reflect.Type = {
        let type U = b;
        reflect.Type.of<Wrapper<U>>()
    }
    [first, second]
}
"#;

/// Sibling blocks reuse one frame slot for their bindings. A type derived
/// from the first binding is built where it is loaded, as the interpreter
/// evaluates `LoadType`, so the second block's rebinding of the slot does
/// not reach it.
#[test]
fn a_derived_type_keeps_its_binding_past_a_sibling_rebinding() {
    let module = compile(SIBLING_BINDINGS);
    let body = body_of(&module, "user.sibling_bindings")
        .split_whitespace()
        .collect::<String>();
    let built = body.find("vm.type_value(").expect("the type is built");
    let rebound = body.rfind("vm.bind_type(").expect("the slot is rebound");
    assert!(built < rebound, "{body}");
}

#[test]
fn recursive_calls_poll_and_root_live_values() {
    let module = compile(
        r#"
function fib(n: int) -> int {
  if (n < 2) { n } else { fib(n - 1) + fib(n - 2) }
}
function left(s: string, n: int) -> string {
  if (n == 0) { s } else { right(s, n - 1) }
}
function right(s: string, n: int) -> string {
  if (n == 0) { s } else { left(s, n - 1) }
}
function caller(s: string, n: int) -> string { left(s, n) + s }
function plain(n: int) -> int { n + 1 }
"#,
    );
    for name in ["user.fib", "user.left", "user.right"] {
        let body = body_of(&module, name);
        assert!(body.contains("CALLS.with("), "{body}");
        assert!(body.contains("wrapping_add(1)"), "{body}");
        assert!(body.contains("vm.should_yield()"), "{body}");
        assert!(body.contains("vm.suspend(Step::Yield)"), "{body}");
    }
    for name in ["user.left", "user.right", "user.caller"] {
        let body = body_of(&module, name);
        assert!(body.contains("vm.set_slot(depth,"), "{body}");
        assert!(body.contains("vm.slot(depth,"), "{body}");
    }
    for name in ["user.caller", "user.plain"] {
        let body = body_of(&module, name);
        assert!(!body.contains("CALLS.with("), "{body}");
    }
    syn::parse_file(&module.source).expect("generated module parses as Rust");
}

