//! The BAML-to-Rust backend admits the compute-shaped subset and keeps the
//! rest bytecode, with a reason, and its output is well-formed Rust.
//!
//! Rust rather than a BAML test because the backend's output is Rust source
//! and its admission table is compiler-side data a program cannot observe.

use baml_compiler2_emit::OptLevel;
use baml_db::rust_backend::{Rejection, RustModule, compile_program};
use bex_vm_types::{Instruction, Object, ObjectIndex, RealizedTy, types::Program};

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
        Rejection::Invalid(reason) if reason == "the function's bytecode was emitted from other MIR"
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
    for name in ["user.fib", "user.<lambda(uses_closure, 0)>"] {
        let body = body_of(&module, name);
        assert!(!body.contains("vm.suspend("), "{body}");
        assert!(!body.contains("vm.should_yield()"), "{body}");
    }
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
    // Compacted: whitespace out, and a trailing comma `prettyplease` adds to
    // a wrapped argument list with it, so the formatting of a long entry
    // cannot fail the check.
    let compact: String = module
        .source
        .split_whitespace()
        .collect::<String>()
        .replace(",)", ")")
        .replace(",]", "]");
    let fingerprint = format!("fingerprint:{:?}", fingerprint(&program))
        .split_whitespace()
        .collect::<String>();
    assert_eq!(compact.matches(&fingerprint).count(), 1);
    for function in &module.functions {
        // Compacted like the source: an impl body's name has spaces in it
        // (`<(ai.Prompt as baml.ToString)>.to_string`).
        let entry: String = format!(
            "CompiledEntry{{object:{},name:{:?},arity:{},body:{},\
             slots:{},cause_slot:{:?},",
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
    // Every compiled call gets a frame (depth limit, traces, telemetry).
    assert!(compact.contains("vm.call_compiled("));
    // An error leaving a compiled function records the PC it was raised at.
    assert!(compact.contains("vm.note_pc(depth,pc)"));
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

/// Maps, enum variants, bigints, byte arrays, a top-level `client`,
/// structural type tests, a class-tag `match` and `Package.current` compile,
/// each to the runtime helper that does what the bytecode instruction does.
#[test]
fn small_constructs_compile_to_their_runtime_helpers() {
    let module = compile(
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
    );
    let compact = |name: &str| {
        body_of(&module, name)
            .split_whitespace()
            .collect::<String>()
    };

    // A string-keyed map: the literal allocates, and every key operation is
    // a direct builtin call (no suspension). Any other key hashes through its
    // `baml.Hash` implementation, which runs BAML code, so an int-keyed map's
    // operations suspend.
    assert!(!compact("user.words").contains("vm.suspend(Step::Call"));
    assert!(compact("user.squares").contains("vm.suspend(Step::Call"));
    let words = compact("user.words");
    assert!(
        words.contains("vm.new_map(RealizedTy::String,RealizedTy::Int)"),
        "{words}"
    );
    assert!(words.contains("vm.call_builtin(builtin_"), "{words}");
    // A class-keyed map may run a user `hash`/`eq`: its index suspends.
    assert!(compact("user.keyed").contains("vm.suspend("));

    let green = compact("user.green");
    assert!(
        green.contains("vm.new_variant(vm.idx_to_ptr(ObjectIndex::from_raw("),
        "{green}"
    );
    assert!(green.contains(")),1);"), "variant #1 is Green: {green}");

    // A bigint literal is the program's own object; the product goes through
    // the VM's generic operator.
    let big = compact("user.big");
    assert!(!big.contains("vm.string_value("), "{big}");
    assert!(big.contains("vm.binop(BinOp::Mul,"), "{big}");

    // A byte array literal is its pooled template deep-copied.
    let bytes = compact("user.bytes");
    assert!(bytes.contains("vm.call_builtin(builtin_"), "{bytes}");
    assert!(
        module.source.contains("\"baml.deep_copy\""),
        "deep_copy is bound by key"
    );
    assert!(bytes.contains("vm.store_array_element("), "{bytes}");

    let model = compact("user.model");
    assert!(
        model.contains("vm.load_global(vm.idx_to_ptr(ObjectIndex::from_raw("),
        "{model}"
    );

    // Structural tests read the function's own pooled type constants; the
    // `int` leaf is a tag compare.
    let kind = compact("user.kind");
    assert_eq!(kind.matches("vm.is_type_constant(").count(), 2, "{kind}");
    assert!(kind.contains("bex_vm::compiled_rt::type_tag("), "{kind}");

    // A class-tag switch matches the classes' linker-assigned tags.
    let area = compact("user.area");
    assert!(area.contains("matchd"), "{area}");
    assert!(area.contains("i64=>{"), "{area}");
    assert!(area.contains("bex_vm::compiled_rt::type_tag("), "{area}");

    let here = compact("user.here");
    assert!(here.contains("vm.current_package("), "{here}");
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
/// at compile time resolves it at run time, through the VM, and can suspend
/// to request the call; interface fields and method values resolve inline.
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
        let body = body_of(&module, name)
            .split_whitespace()
            .collect::<String>();
        // Only the calls suspend; a method value, a field and a call on a
        // primitive (devirtualized by MIR) resolve in place.
        assert_eq!(
            body.contains("vm.suspend(Step::Call"),
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
        total.contains("matchvm.call_inline(callee,&[") && total.contains("Ok(Some(v))=>Ok(v)"),
        "{total}"
    );
    assert!(
        total.contains("vm.push_virtual_args(callee,Some(&[None]),&["),
        "{total}"
    );
    assert!(
        total.contains("vm.suspend(Step::Call{callee,argc})"),
        "{total}"
    );
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

/// An implementation may declare optionals its interface method does not.
/// A site that names none still hands the VM its layout, so the arguments
/// are laid over the resolved implementation's slots, its extra optionals
/// omitted, as the interpreter's call path lays them.
#[test]
fn a_virtual_call_lays_its_arguments_over_the_implementation() {
    let module = compile(
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
    );
    let body = body_of(&module, "user.grow_by")
        .split_whitespace()
        .collect::<String>();
    assert!(body.contains("vm.call_inline(callee,&["), "{body}");
    assert!(
        body.contains("vm.push_virtual_args(callee,Some(&[None,None]),&["),
        "{body}"
    );
    syn::parse_file(&module.source).expect("generated module parses as Rust");
}

/// `a && b` with a non-bool `b` stores `b` into the expression's `bool`
/// result and coerces it there; until then the result holds an `int`, so it
/// is held as a `Value`, not read as a `bool`.
#[test]
fn a_short_circuit_holds_its_uncoerced_operand() {
    let module = compile(
        r#"
function tr_and(a: string?, b: int) -> bool {
  a && b
}
"#,
    );
    let body = body_of(&module, "user.tr_and")
        .split_whitespace()
        .collect::<String>();
    assert!(body.contains("vm.is_truthy("), "{body}");
    assert!(!body.contains("expect_bool(Value::int("), "{body}");
    syn::parse_file(&module.source).expect("generated module parses as Rust");
}

/// A `$trace` attachment the VM refuses (a reservation attached twice)
/// raises inside the block that makes the call, so the block's handler
/// catches it, as bytecode's `SetCallTrace` raises in the handler's region.
#[test]
fn a_refused_trace_attachment_is_caught_by_the_enclosing_handler() {
    let module = compile(
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
    );
    let body = body_of(&module, "user.reservation_reuse_fails")
        .split_whitespace()
        .collect::<String>();
    assert!(body.contains("(matchvm.set_call_trace("), "{body}");
    syn::parse_file(&module.source).expect("generated module parses as Rust");
}

/// A call to a pass-through function (an LLM function's `@spec` companion)
/// is requested from the VM, whose call path holds the call's trace controls
/// for the callee and ends them, and its span activations, on return.
#[test]
fn a_pass_through_callee_is_called_through_the_vm() {
    let module = compile(
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
    );
    let body = body_of(&module, "user.has_spec");
    assert!(body.contains("vm.suspend("), "{body}");
    assert!(!body.contains("vm.call_compiled"), "{body}");
}

/// A declaration hook compiles as its bytecode prologue: a built-in mode
/// resolves the hook the VM's call path left pending for the frame; a hook
/// function gets its settings from the VM, runs on the VM's call path (it
/// suppresses hooks under it and attributes it to the target's caller), and
/// its decision is applied before the body. The stdlib's hidden functions
/// (`String.length`, the iterators) compile, so their callers enter them
/// directly.
#[test]
fn declaration_trace_hooks_compile() {
    let module = compile(
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
    );
    for (name, mode) in [
        ("user.hidden", "Hidden"),
        ("user.timing", "Timing"),
        ("user.spanned", "Span"),
        ("user.empty", "EmptySpan"),
    ] {
        let body = body_of(&module, name);
        assert!(
            body.contains(&format!(
                "vm.builtin_trace_hook(depth, bex_vm::compiled_rt::TraceHookMode::{mode})?"
            )),
            "{body}"
        );
        assert!(!body.contains("vm.suspend("), "{body}");
    }
    for (name, with_settings) in [
        ("user.hooked", true),
        ("user.hooked_without_settings", false),
    ] {
        let body = body_of(&module, name);
        assert!(
            body.contains(&format!("vm.begin_trace_hook(depth, {with_settings})?")),
            "{body}"
        );
        assert!(body.contains("vm.finish_trace_hook(depth, "), "{body}");
        assert!(body.contains("vm.suspend(Step::Call {"), "{body}");
        assert!(!body.contains("vm.call_compiled("), "{body}");
    }
    let admitted: Vec<&str> = module
        .functions
        .iter()
        .map(|f| f.display.as_str())
        .collect();
    for name in [
        "baml.String.length",
        "baml.iter.Range.new",
        "baml.<(baml.iter.Range as baml.iter.Iterator)>.next",
        "baml.<(baml.iter.Range as baml.iter.Iterable)>.iter",
    ] {
        assert!(
            admitted.contains(&name),
            "{name} should compile; got {admitted:?}"
        );
    }
    // `String.length` is entered directly; the `Range` dispatch runs in place.
    let total = body_of(&module, "user.total");
    assert!(total.contains(".call_compiled("), "{total}");
    assert!(total.contains(".call_inline("), "{total}");
    syn::parse_file(&module.source).expect("generated module parses as Rust");
}

/// An array's iterator that the function returns leaves the body, so it is
/// the `ArrayIterator` the dispatch builds rather than the index walk a
/// local loop compiles to: a caller's `next` resolves against it.
#[test]
fn a_returned_array_iterator_is_dispatched() {
    let module = compile(
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
    );
    let body = body_of(&module, "user.iter_of")
        .split_whitespace()
        .collect::<String>();
    assert!(body.contains("vm.virtual_target("), "{body}");
    assert!(!body.contains("i0=0;"), "{body}");
    syn::parse_file(&module.source).expect("generated module parses as Rust");
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
    syn::parse_file(&module.source).expect("generated module parses as Rust");
}

/// An array's iterator used as a value (passed on, adapted, or consumed
/// after the source changed) is the `ArrayIterator` the dispatch builds, so
/// the function compiles with that call dispatched rather than being
/// rejected. A loop in the same body that alone consumes its own iterator
/// still walks by index.
#[test]
fn an_escaping_array_iterator_is_dispatched() {
    let module = compile(
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
    );
    for name in ["user.passed", "user.chained", "user.lazy"] {
        let body = body_of(&module, name)
            .split_whitespace()
            .collect::<String>();
        assert!(body.contains("vm.virtual_target("), "{name}: {body}");
        assert!(!body.contains("vm.container_len("), "{name}: {body}");
        assert!(!body.contains("vm.load_array_element("), "{name}: {body}");
    }
    let body = body_of(&module, "user.mixed")
        .split_whitespace()
        .collect::<String>();
    assert!(body.contains("vm.virtual_target("), "{body}");
    assert!(body.contains("vm.container_len("), "{body}");
    assert!(body.contains("vm.load_array_element("), "{body}");
    assert!(
        !module
            .rejected
            .iter()
            .any(|(_, why)| matches!(why, Rejection::Unsupported(r) | Rejection::Invalid(r) if r == "array iterator used as a value")),
        "{:?}",
        module.rejected
    );
    syn::parse_file(&module.source).expect("generated module parses as Rust");
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

/// Generic functions compile polymorphically: one body per function that
/// reads `T` off its frame (`vm.frame_type_arg`) and builds the types it
/// derives from it for its `is T` tests, `T[]` arrays,
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
    assert!(
        is_int_t.contains("vm.frame_type_arg(depth,0)?"),
        "{is_int_t}"
    );
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

/// Closures compile: a captured local is a cell in a Rust variable, a
/// captured parameter is wrapped at entry, a loop header's binding carries
/// its value into the next iteration's cell, a lambda is a unit of its own
/// reading its captures off its closure, and a closure is made over the
/// lambda's own object.
#[test]
fn closures_compile() {
    let module = compile(CLOSURES);
    let admitted: Vec<&str> = module
        .functions
        .iter()
        .map(|f| f.display.as_str())
        .collect();
    for name in [
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
    ] {
        assert!(
            admitted.contains(&name),
            "{name} should compile; got {admitted:?} / {:?}",
            module.rejected
        );
    }
    // A lambda's entry names its own object, as the loader renders it, with
    // the package it belongs to in the report.
    let lambda = module
        .functions
        .iter()
        .find(|f| f.display == "user.<lambda(cap_local, 0)>")
        .expect("compiles");
    assert_eq!(lambda.name, ".<lambda(cap_local, 0)>");
    assert_eq!(lambda.arity, 1);
    assert!(lambda.symbol.starts_with("user__lambda_cap_local__0__"));

    let compact = |name: &str| {
        body_of(&module, name)
            .split_whitespace()
            .collect::<String>()
    };
    // The captured local's cell, made where the binding is created, read and
    // written through, and handed to the closure over the lambda's object.
    let cap_local = compact("user.cap_local");
    assert!(
        cap_local.contains("=vm.new_cell(Value::NULL);"),
        "{cap_local}"
    );
    assert!(cap_local.contains("vm.cell_store(l"), "{cap_local}");
    assert!(cap_local.contains("vm.cell_load(l"), "{cap_local}");
    assert!(
        cap_local.contains(&format!(
            "vm.make_closure(vm.idx_to_ptr(ObjectIndex::from_raw({})),vec![l",
            lambda.object
        )),
        "{cap_local}"
    );
    // A call through the closure: direct when its body is compiled,
    // through the VM otherwise.
    assert!(
        cap_local.contains("vm.closure_compiled_body(callee,1)"),
        "{cap_local}"
    );
    assert!(
        cap_local.contains("vm.call_closure_compiled(callee,body,&["),
        "{cap_local}"
    );
    assert!(
        cap_local.contains("vm.suspend(Step::Call{callee,argc:1})"),
        "{cap_local}"
    );
    // The lambda reads and writes the capture's cell off its frame, and
    // keeps the int it holds unboxed.
    let add = compact("user.<lambda(cap_local, 0)>");
    assert!(add.contains("vm.capture_cell(depth,0)?"), "{add}");
    assert!(add.contains("expect_int(vm.cell_load(t"), "{add}");
    assert!(add.contains("vm.add_i64("), "{add}");
    assert!(add.contains("vm.cell_store(t"), "{add}");
    // A captured parameter is wrapped at entry.
    let cap_param = compact("user.cap_param");
    assert!(
        cap_param.contains("letmutl1:Value=vm.new_cell(args[0]);"),
        "{cap_param}"
    );
    // A loop header's binding is carried into the next iteration's cell.
    let loop_cells = compact("user.loop_cells");
    assert!(loop_cells.contains("vm.cell_load(l"), "{loop_cells}");
    assert!(loop_cells.contains("=vm.new_cell(t"), "{loop_cells}");
    // A nested lambda forwards a capture's cell to the closure it makes.
    let outer = compact("user.<lambda(nested, 0)>");
    assert!(outer.contains("vm.make_closure("), "{outer}");
    assert!(outer.contains("vm.capture_cell(depth,0)?"), "{outer}");
    syn::parse_file(&module.source).expect("generated module parses as Rust");
}

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

/// Types the backend does not build from their parts are the constant
/// bytecode's own `LoadType` reads; a dispatch instantiated from the frame
/// resolves every time instead of through the site's cache; a method value
/// is a bound method.
#[test]
fn runtime_types_dispatch_and_bound_methods_compile() {
    let module = compile(RUNTIME_TYPES);
    let compact = |name: &str| {
        body_of(&module, name)
            .split_whitespace()
            .collect::<String>()
    };
    let describe = compact("user.generic_describe");
    assert!(
        describe.contains("vm.virtual_target_uncached("),
        "{describe}"
    );
    assert!(!describe.contains("staticS"), "{describe}");
    let fns = compact("user.fns");
    assert!(fns.contains("vm.frame_type_constant(depth,"), "{fns}");
    let is_tree = compact("user.is_tree");
    assert!(is_tree.contains("vm.is_type_constant("), "{is_tree}");
    let bound = compact("user.bound");
    assert!(bound.contains("vm.make_bound_method("), "{bound}");
    syn::parse_file(&module.source).expect("generated module parses as Rust");
}

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

/// A `let type` bound check compiles to the VM's own check against the
/// constraint, loaded as bytecode loads it.
#[test]
fn let_type_bound_checks_compile() {
    let module = compile(LET_TYPE_BOUND);
    let body = body_of(&module, "user.bind_named")
        .split_whitespace()
        .collect::<String>();
    assert!(body.contains("vm.bound_satisfied("), "{body}");
    syn::parse_file(&module.source).expect("generated module parses as Rust");
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
    syn::parse_file(&module.source).expect("generated module parses as Rust");
}

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

/// A type bound at run time (`let type T = t`) is read from the frame slot
/// the binding wrote, at the call that passes it on, after the binding; the
/// same for an interface dispatch and a virtual function value, which cannot
/// use the site's receiver-only cache.
#[test]
fn runtime_bound_type_arguments_compile() {
    let module = compile(RUNTIME_BOUND_ARGUMENTS);
    let compact = |name: &str| {
        body_of(&module, name)
            .split_whitespace()
            .collect::<String>()
    };
    let call = compact("user.bound_call");
    let bound = call
        .find("vm.bind_type(depth,0,")
        .expect("the slot is bound");
    let read = call
        .rfind("vm.frame_type_arg(depth,0)?")
        .expect("the type is read from the frame");
    assert!(bound < read, "{call}");
    assert!(call.contains("vm.call_compiled_with_types("), "{call}");
    assert!(!call.contains("vm.type_operand("), "{call}");
    let dispatch = compact("user.bound_dispatch");
    assert!(dispatch.contains("vm.bind_type(depth,0,"), "{dispatch}");
    assert!(
        dispatch.contains("vm.virtual_target_uncached("),
        "{dispatch}"
    );
    assert!(!dispatch.contains("staticS"), "{dispatch}");
    assert!(dispatch.contains("vm.make_virtual_function("), "{dispatch}");
    syn::parse_file(&module.source).expect("generated module parses as Rust");
}

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

/// An interface type argument is built from the program's interface object,
/// in a call's type arguments and in an instance test.
#[test]
fn interface_type_arguments_compile() {
    let module = compile(INTERFACE_ARGUMENTS);
    let body = body_of(&module, "user.tagged_speaker")
        .split_whitespace()
        .collect::<String>();
    assert!(body.contains("vm.interface_type_at("), "{body}");
    assert!(body.contains("vm.is_instance_of_at("), "{body}");
    assert!(!body.contains("vm.frame_type_constant("), "{body}");
    syn::parse_file(&module.source).expect("generated module parses as Rust");
}

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

/// Loops poll at their headers; a call to a function that may suspend roots
/// the caller's live values on the VM stack for its duration.
#[test]
fn compiled_code_polls_and_roots_where_it_may_suspend() {
    let module = compile(GC_CHECKS);
    let compact = |name: &str| {
        body_of(&module, name)
            .split_whitespace()
            .collect::<String>()
    };
    let sum_to = compact("user.sum_to");
    assert!(sum_to.contains("vm.should_yield()"), "{sum_to}");
    assert!(sum_to.contains("vm.suspend(Step::Yield)"), "{sum_to}");
    assert!(!sum_to.contains("vm.set_slot("), "{sum_to}");
    let build = compact("user.build");
    assert!(
        build.contains("iterations=iterations.wrapping_add(1);"),
        "{build}"
    );
    assert!(
        build.contains("iterations%16==0&&vm.should_yield()"),
        "{build}"
    );
    assert!(build.contains("vm.suspend(Step::Yield)"), "{build}");
    let keep = compact("user.keep");
    assert!(keep.contains("vm.set_slot(depth,0,l1);"), "{keep}");
    assert!(keep.contains("l1=vm.slot(depth,0);"), "{keep}");
    let main = compact("user.main");
    assert!(!main.contains("vm.set_slot("), "{main}");
    syn::parse_file(&module.source).expect("generated module parses as Rust");
}
