//! Compiler admission, generated Rust structure, and bytecode identity checks.

use baml_compiler2_emit::OptLevel;
use baml_db::rust_backend::{Rejection, RustModule, compile_program};
use bex_vm_types::{
    Instruction, Object, ObjectIndex,
    types::{Function, FunctionIdentity, Program},
};

fn lower(db: &baml_db::ProjectDatabase, opt: OptLevel) -> Program {
    baml_db::compile_program(db, db.workspace_root().unwrap(), opt).unwrap()
}

fn fingerprint(program: &Program) -> [u8; 32] {
    baml_artifact::encode_with_digest(baml_artifact::ArtifactKind::Program, program)
        .unwrap()
        .1
}

fn render(
    db: &baml_db::ProjectDatabase,
    program: &Program,
    opt: OptLevel,
    entries: Option<&[usize]>,
) -> RustModule {
    compile_program(
        db,
        db.workspace_root().unwrap(),
        program,
        opt,
        "program.bexa",
        fingerprint(program),
        entries,
    )
    .expect("generated module renders")
}

#[test]
fn initializers_and_global_calls_at_every_opt_level() {
    let db = baml_test_support::setup_multi_file_db(&[
        (
            "globals.baml",
            r#"
const FORWARD = LATER + 1;
let LATER = 41;
const CAPTURED = { let offset = FORWARD; (n: int) -> int { n + offset } };
function fail() -> int throws string { throw "initializer" }
const RECOVERED = fail() catch (e) { string => 7 };
function main() -> int { CAPTURED(RECOVERED) }
test "initializer value" { assert.equal(main(), 49) }
"#,
        ),
        (
            "ns_other/globals.baml",
            r#"
const LATER = [1, 2, 3];
test "other initializer" { assert.equal(LATER.length(), 3) }
"#,
        ),
        (
            "ns_calls/global_calls.baml",
            include_str!("../rust_diff/baml_src/global_calls.baml"),
        ),
    ]);
    for opt in [OptLevel::Zero, OptLevel::One, OptLevel::Two] {
        let program = lower(&db, opt);
        let module = render(&db, &program, opt, None);
        let mut helpers = 0;
        let mut chainers = 0;
        for (object, function) in user_functions(&program) {
            assert!(
                module.functions.iter().any(|f| f.object == object),
                "{:?} should compile at {opt:?}: {:?}",
                function.identity,
                module.rejected
            );
            match function.identity {
                FunctionIdentity::InitLet(_) => {
                    helpers += 1;
                    assert!(
                        !function.bytecode.block_pcs.is_empty(),
                        "initializer helpers carry the same MIR positions as ordinary functions"
                    );
                }
                FunctionIdentity::Init | FunctionIdentity::InitTest => {
                    chainers += 1;
                }
                _ => {}
            }
        }
        assert_eq!(helpers, 10);
        assert_eq!(chainers, 2);
        assert!(body_of(&module, "user.calls.GlobalCalls").contains("vm.indirect_callee("));
        syn::parse_file(&module.source).expect("generated initializers parse as Rust");
    }
}

#[test]
fn entry_points_keep_initializer_helpers_and_their_lambda_callees() {
    let db = baml_test_support::setup_test_db(
        r#"
function only_from_initializer(n: int) -> int { n + 1 }
const CALLBACK = (n: int) -> int { only_from_initializer(n) };
const UNUSED = only_from_initializer(8);
function main() -> int { 42 }
function dead() -> int { 0 }
function only_from_test() -> int { 9 }
test "registered initializer" { assert.equal(only_from_test(), 9) }
"#,
    );
    let program = lower(&db, OptLevel::Two);
    let entries = [program.rendered_callables()["user.main"].object.raw()];
    let module = render(&db, &program, OptLevel::Two, Some(&entries));
    let dead = program.rendered_callables()["user.dead"].object.raw();
    for (object, _) in user_functions(&program).filter(|(object, _)| *object != dead) {
        assert!(
            module.functions.iter().any(|f| f.object == object),
            "object {object} missing"
        );
    }
    assert!(matches!(
        rejection(&module, "user.dead"),
        Rejection::Unreached
    ));
}

#[test]
fn indirect_global_call_load_order_matches_bytecode() {
    let db = baml_test_support::setup_test_db(
        r#"
const CALLBACK = (value: int) -> int { value };
const ARGUMENT = 7;
function invoke() -> int { CALLBACK(ARGUMENT) }
"#,
    );
    for opt in [OptLevel::Zero, OptLevel::One, OptLevel::Two] {
        let program = lower(&db, opt);
        let module = render(&db, &program, opt, None);
        let compiled = module
            .functions
            .iter()
            .find(|f| f.name == "user.invoke")
            .unwrap();
        let Object::Function(function) = &program.objects[ObjectIndex::from_raw(compiled.object)]
        else {
            unreachable!()
        };
        let bytecode: Vec<_> = function
            .bytecode
            .instructions
            .iter()
            .filter_map(|instruction| match instruction {
                Instruction::LoadGlobal(slot) => Some(slot.raw()),
                _ => None,
            })
            .collect();
        assert_eq!(bytecode.len(), 2);
        let body = body_of(&module, "user.invoke");
        let mut native: Vec<_> = bytecode.iter().map(|slot| {
            let load = format!("vm.load_global_checked(vm.record_object(fr, ObjectIndex::from_raw({})), {slot})", compiled.object);
            (body.find(&load).expect("global is loaded"), *slot)
        }).collect();
        native.sort_unstable();
        assert_eq!(
            native.iter().map(|(_, slot)| *slot).collect::<Vec<_>>(),
            bytecode,
            "{opt:?}: {body}"
        );
    }
}

#[test]
#[ignore = "explicit full-corpus admission check"]
fn corpus_bytecode_functions_are_all_admitted() {
    use baml_tests::engine::TestDbExt as _;

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("baml_src");
    let mut files = baml_db::discover_baml_files(&path);
    files.sort();
    assert!(!files.is_empty());
    let mut db = baml_db::ProjectDatabase::new();
    db.workspace(&path);
    for file in files {
        db.file(
            &file,
            &std::fs::read_to_string(&file)
                .unwrap()
                .replace("\r\n", "\n"),
        );
    }
    let program = lower(&db, OptLevel::Two);
    let module = render(&db, &program, OptLevel::Two, None);
    assert!(module.rejected.is_empty(), "{:?}", module.rejected);
    let admitted: std::collections::BTreeSet<_> =
        module.functions.iter().map(|f| f.object).collect();
    assert_eq!(
        admitted.len(),
        module.functions.len(),
        "no duplicate entries"
    );
    let bytecode: std::collections::BTreeSet<_> = program
        .objects
        .iter()
        .enumerate()
        .filter_map(|(object, item)| match item {
            Object::Function(f) if matches!(f.kind, bex_vm_types::FunctionKind::Bytecode) => {
                Some(object)
            }
            _ => None,
        })
        .collect();
    assert_eq!(admitted, bytecode);
    eprintln!("{} bytecode functions admitted", admitted.len());
}

const FIXTURE: &str = r#"
class Point { x int y int }
function sum(xs: int[]) -> int { let total = 0; let i = 0; while (i < xs.length()) { total = total + xs[i]; i = i + 1; } total }
function fib(n: int) -> int { if (n < 2) { n } else { fib(n - 1) + fib(n - 2) } }
function manhattan(p: Point, q: Point) -> int { let dx = p.x - q.x; let dy = p.y - q.y; (if (dx < 0) { 0 - dx } else { dx }) + (if (dy < 0) { 0 - dy } else { dy }) }
function safe_div(a: int, b: int) -> int { (a / b) catch (e) { baml.panics.DivisionByZero => -1 } }
function mean(xs: float[]) -> float { let total = 0.0; let i = 0; while (i < xs.length()) { total = total + xs[i]; i = i + 1; } total / 2.0 }
function uses_closure(xs: int[]) -> int[] { xs.map((x: int) => { x * 2 }) }
function calls_rejected(xs: int[]) -> int { uses_closure(xs).length() }
function generic<T>(x: T) -> T { x }
function for_sum(xs: int[]) -> int { let total = 0; for (let v in xs) { total += v; } total }
function build(n: int) -> int[] { let out: int[] = []; let i = 0; while (i < n) { out.push(i); i += 1; } out }
function origin() -> Point { Point { x: 0, y: 0 } }
function label(n: int) -> string { n.to_string() }
function with_default(x: int, y: int = 5) -> int { x + y }
function greet(name: string) -> bool { name == "build" }
function main() -> string { let p = Point { x: 1, y: 2 }; let q = Point { x: 4, y: 6 }; `${sum([1, 2, 3])}|${fib(10)}|${manhattan(p, q)}|${safe_div(7, 0)}|${mean([1.0, 3.0])}` }
"#;

fn rejection<'a>(module: &'a RustModule, name: &str) -> &'a Rejection {
    &module
        .rejected
        .iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("{name} should be rejected"))
        .1
}

fn user_functions(program: &Program) -> impl Iterator<Item = (usize, &Function)> {
    program.packages[program.root as usize]
        .objects
        .clone()
        .filter_map(
            |object| match &program.objects[ObjectIndex::from_raw(object as usize)] {
                Object::Function(function)
                    if matches!(function.kind, bex_vm_types::FunctionKind::Bytecode) =>
                {
                    Some((object as usize, function.as_ref()))
                }
                _ => None,
            },
        )
}

fn body_of<'a>(module: &'a RustModule, name: &str) -> &'a str {
    let function = module
        .functions
        .iter()
        .find(|f| f.display == name)
        .unwrap_or_else(|| panic!("{name} should compile: {:?}", module.rejected));
    let start = module
        .source
        .find(&format!("pub fn {}_d(", function.symbol))
        .unwrap();
    let source = &module.source[start..];
    let end = source
        .match_indices('\n')
        .map(|(at, _)| at + 1)
        .find(|&at| {
            ["pub ", "fn ", "static ", "const ", "#[", "use ", "type "]
                .iter()
                .any(|item| source[at..].starts_with(item))
        })
        .unwrap_or(source.len());
    &source[..end]
}

fn assert_admitted(module: &RustModule, names: &[&str]) {
    for name in names {
        assert!(
            module.functions.iter().any(|f| f.display == *name),
            "{name} should compile; rejected: {:?}",
            module.rejected
        );
    }
}

type ShapeCase = (
    &'static str,
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
);

// Construct/function, source, required markers, forbidden markers; all user bodies must compile.
#[rustfmt::skip]
const CONSTRUCTS: &[ShapeCase] = &[
    ("compute.fib", FIXTURE,
        &["vm.should_yield_at_call()", "vm.suspend(Step::Yield)"], &[]),
    ("containers.Color", r#"enum Color { Red Green }"#,
        &[], &[]),
    ("containers.Circle", r#"class Circle { r: int }"#,
        &[], &[]),
    ("containers.Square", r#"class Square { s: int }"#,
        &[], &[]),
    ("containers.Tri", r#"class Tri { a: int }"#,
        &[], &[]),
    ("containers.Hex", r#"class Hex { h: int }"#,
        &[], &[]),
    ("containers.declarations", r#"client Probe = openai.ResponsesClient.new(model = "m", api_key = "k", base_url = "http://localhost:1/v1");"#,
        &[], &[]),
    ("containers.words", r#"function words(xs: string[]) -> map<string, int> { let counts: map<string, int> = {}; for (let x in xs) { counts[x] = (counts.get(x) ?? 0) + 1; } counts }"#,
        &[], &[]),
    ("containers.squares", r#"function squares(n: int) -> map<int, int> { let m: map<int, int> = {}; let i = 0; while (i < n) { m[i] = i * i; i += 1; } m }"#,
        &[], &[]),
    ("containers.keyed", r#"function keyed(m: map<Circle, int>, c: Circle) -> int { m[c] }"#,
        &[], &[]),
    ("containers.green", r#"function green() -> Color { Color.Green }"#,
        &[], &[]),
    ("containers.big", r#"function big(n: bigint) -> bigint { n * 123456789012345678901234567890n }"#,
        &[], &[]),
    ("containers.bytes", r#"function bytes() -> int { let b = b"\x01\x02"; b[0] = 9; b[0] }"#,
        &[], &[]),
    ("containers.model", r#"function model() -> string { Probe.model }"#,
        &[], &[]),
    ("containers.kind", r#"function kind(x: int[] | string[] | map<string, int> | int) -> string { if (x is int[]) { "ints" } else if (x is map<string, int>) { "map" } else if (x is int) { "int" } else { "strings" } }"#,
        &[], &[]),
    ("containers.area", r#"function area(s: Circle | Square | Tri | Hex) -> int { match (s) { Circle => s.r, Square => s.s, Tri => s.a, Hex => s.h, } }"#,
        &[], &[]),
    ("containers.here", r#"function here() -> reflect.Package { reflect.Package.current() }"#,
        &[], &[]),
    ("interface_defaults.Grows", r#"interface Grows { function grow(self, by: int) -> int throws never }"#,
        &[], &[]),
    ("interface_defaults.Plot", r#"class Plot { side int implements Grows { function grow(self, by: int, extra: int = 1) -> int throws never { self.side * by + extra } } }"#,
        &[], &[]),
    ("interface_defaults.grow_by", r#"function grow_by(g: Grows, by: int) -> int { g.grow(by) }"#,
        &[], &[]),
    ("truthiness.tr_and", r#"function tr_and(a: string?, b: int) -> bool { a && b }"#,
        &[], &[]),
    ("reservations.own_id", r#"function own_id() -> trace.SpanId? { trace.current_span_id() }"#,
        &[], &[]),
    ("reservations.reservation_reuse_fails", r#"function reservation_reuse_fails() -> bool { let reserved = trace.span().reserve(); let alias = reserved; own_id($trace = reserved); { own_id($trace = alias); false } catch_all_panics (error) { _ => true, } }"#,
        &[], &[]),
    ("llm_spec.declarations", r#"client TestClient = openai.ResponsesClient.new( model = "gpt-4o-mini", api_key = "test-key", base_url = "http://localhost:1234", );"#,
        &[], &[]),
    ("llm_spec.Describe", r#"function Describe(topic: string) -> string { client: TestClient prompt: `Describe ${topic}.` }"#,
        &[], &[]),
    ("llm_spec.has_spec", r#"function has_spec(topic: string) -> bool { let spec = Describe@spec(topic); true }"#,
        &[], &[]),
    ("tracing.hidden", r#"/// baml:$trace=trace.hidden
function hidden(n: int) -> int { n + 1 }"#,
        &[], &[]),
    ("tracing.timing", r#"/// baml:$trace=trace.timing
function timing(n: int) -> int { n + 2 }"#,
        &[], &[]),
    ("tracing.spanned", r#"/// baml:$trace=trace.span
function spanned(n: int) -> int { n + 3 }"#,
        &[], &[]),
    ("tracing.empty", r#"/// baml:$trace=trace.empty_span
function empty(n: int) -> int { n + 4 }"#,
        &[], &[]),
    ("tracing.decide", r#"function decide(n: int, settings: trace.Settings) -> trace.Options? throws never { trace.hidden() }"#,
        &[], &[]),
    ("tracing.hooked", r#"/// baml:$trace=decide
function hooked(n: int) -> int { n + 5 }"#,
        &[], &[]),
    ("tracing.plain_hook", r#"function plain_hook() -> trace.Options throws never { trace.hidden() }"#,
        &[], &[]),
    ("tracing.hooked_without_settings", r#"/// baml:$trace=plain_hook
function hooked_without_settings(n: int) -> int { n + 6 }"#,
        &[], &[]),
    ("tracing.total", r#"function total(words: string[]) -> int { let total = 0; let i = 0; while (i < words.length()) { total += words[i].length(); i += 1; } for (let k in baml.iter.Range.new(0, 3)) { total += k; } total }"#,
        &[], &[]),
    ("iterator_return.iter_of", r#"function iter_of(xs: int[]) -> baml.iter.Iterator<Item = int, Error = never> { xs.iter() }"#,
        &[], &[]),
    ("iterator_return.first", r#"function first(xs: int[]) -> int { let total = 0; for (let v in iter_of(xs)) { total += v; } total }"#,
        &[], &[]),
    ("iterators.sum_of", r#"function sum_of(it: baml.iter.Iterator<Item = int, Error = never>) -> int { let total = 0; for (let v in it) { total += v; } total }"#,
        &[], &[]),
    ("iterators.passed", r#"function passed(xs: int[]) -> int { sum_of(xs.iter()) }"#,
        &[], &[]),
    ("iterators.chained", r#"function chained() -> int[] { [1, 2].iter().chain([3, 4]).collect() }"#,
        &[], &[]),
    ("iterators.lazy", r#"function lazy(xs: int[]) -> int { let it = xs.iter(); xs.push(10); sum_of(it) }"#,
        &[], &[]),
    ("iterators.mixed", r#"function mixed(xs: int[]) -> int { let total = 0; for (let v in xs) { total += v; } total + sum_of(xs.iter()) }"#,
        &[], &[]),
    ("interfaces.Shape", r#"interface Shape { function area(self) -> int throws never function scaled(self, by: int) -> int throws never { self.area() * by } }"#,
        &[], &[]),
    ("interfaces.Square", r#"class Square { side int implements Shape { function area(self) -> int throws never { self.side * self.side } } }"#,
        &[], &[]),
    ("interfaces.Rect", r#"class Rect { w int h int implements Shape { function area(self) -> int throws never { self.w * self.h } } }"#,
        &[], &[]),
    ("interfaces.Named", r#"interface Named { name: string }"#,
        &[], &[]),
    ("interfaces.Person", r#"class Person { name: string implements Named {} }"#,
        &[], &[]),
    ("interfaces.total", r#"function total(shapes: Shape[]) -> int { let sum = 0; for (let s in shapes) { sum += s.area(); } sum }"#,
        &[], &[]),
    ("interfaces.scaled", r#"function scaled(s: Shape, by: int) -> int { s.scaled(by) }"#,
        &[], &[]),
    ("interfaces.area_fn", r#"function area_fn(s: Shape) -> () -> int throws never { s.area }"#,
        &[], &[]),
    ("interfaces.rename", r#"function rename(n: Named, to: string) -> string { n.name = to; n.name }"#,
        &[], &[]),
    ("interfaces.describe", r#"function describe(x: int) -> string { x.to_string() }"#,
        &[], &[]),
    ("closures.cap_local", r#"function cap_local(n: int) -> int { let total = 0; let add = (x: int) -> void { total = total + x; }; add(n); add(n); total }"#,
        &[], &[]),
    ("closures.cap_param", r#"function cap_param(n: int) -> int { let bump = () -> int { n = n + 1; n }; bump(); bump() }"#,
        &[], &[]),
    ("closures.loop_cells", r#"function loop_cells() -> int { let total = 0; for (let i = 0; i < 3; i += 1) { let f = () -> int { i }; total = total + f(); } total }"#,
        &[], &[]),
    ("closures.nested", r#"function nested(a: int) -> int { let outer = (b: int) -> int { let inner = (c: int) -> int { a + b + c }; inner(1) }; outer(2) }"#,
        &[], &[]),
    ("closures.stored", r#"function stored(xs: int[]) -> int[] { let double = (x: int) -> int { x * 2 }; xs.map(double) }"#,
        &[], &[]),
    ("closures.main", r#"function main() -> string { `${cap_local(2)}|${cap_param(1)}|${loop_cells()}|${nested(3)}|${stored([1, 2])}` }"#,
        &[], &[]),
    ("runtime_types.Conv", r#"interface Conv<T> { function describe(self) -> string throws never }"#,
        &[], &[]),
    ("runtime_types.Both", r#"class Both { implements Conv<int> { function describe(self) -> string throws never { "as int" } } }"#,
        &[], &[]),
    ("runtime_types.Square", r#"class Square { side int function scaled(self, k: int) -> int { self.side * k } }"#,
        &[], &[]),
    ("runtime_types.Tree", r#"type Tree = int | Tree[]"#,
        &[], &[]),
    ("runtime_types.generic_describe", r#"function generic_describe<T>(c: Conv<T>) -> string throws never { c.describe() }"#,
        &[], &[]),
    ("runtime_types.fns", r#"function fns() -> int { let fs: ((int) -> int throws never)[] = []; fs.push((x: int) -> int { x + 1 }); fs.length() }"#,
        &[], &[]),
    ("runtime_types.is_tree", r#"function is_tree(x: int | string | Tree[]) -> bool { x is Tree }"#,
        &[], &[]),
    ("runtime_types.bound", r#"function bound() -> int { let sq = Square { side: 5 }; let f = sq.scaled; f(3) }"#,
        &[], &[]),
    ("let_type_bound.Named", r#"interface Named { name: string, }"#,
        &[], &[]),
    ("let_type_bound.Person", r#"class Person { name: string, implements Named {} }"#,
        &[], &[]),
    ("let_type_bound.bind_named", r#"function bind_named(t: reflect.Type) -> string { let type T extends Named = t else { return "missed"; }; "bound" }"#,
        &[], &[]),
    ("let_type_bound.main", r#"function main() -> string { bind_named(Person.type()) }"#,
        &[], &[]),
    ("runtime_bound_arguments.Stamper", r#"interface Stamper { function stamp<U>(self, u: U) -> string throws never function emboss<U>(u: U) -> string throws never }"#,
        &[], &[]),
    ("runtime_bound_arguments.Press", r#"class Press { implements Stamper { function stamp<U>(self, u: U) -> string throws never { "stamp " + reflect.Type.of<U>().to_string() } function emboss<U>(u: U) -> string throws never { "emboss " + reflect.Type.of<U>().to_string() } } }"#,
        &[], &[]),
    ("runtime_bound_arguments.id", r#"function id<T>(x: T) -> T { x }"#,
        &[], &[]),
    ("runtime_bound_arguments.bound_call", r#"function bound_call(t: reflect.Type, v: unknown) -> unknown { let type T = t; match (v) { let x: T => id<T>(x), _ => null, } }"#,
        &[], &[]),
    ("runtime_bound_arguments.bound_dispatch", r#"function bound_dispatch(t: reflect.Type, v: unknown) -> string { let p = Press {}; let type U = t; match (v) { let x: U => Stamper.stamp<U>(p, x) + (Press as Stamper).emboss<U>(x), _ => "none", } }"#,
        &[], &[]),
    ("interface_arguments.Speaker", r#"interface Speaker { function speak(self) -> string throws never }"#,
        &[], &[]),
    ("interface_arguments.Tagged", r#"class Tagged<T> { name: string, }"#,
        &[], &[]),
    ("interface_arguments.tagged_speaker", r#"function tagged_speaker(text: string) -> string throws baml.errors.ParseError { let parsed: unknown = baml.sap.parse<Tagged<Speaker>>(text); match (parsed) { let tagged: Tagged<Speaker> => tagged.name, _ => "other", } }"#,
        &[], &[]),
    ("gc_checks.Point", r#"class Point { x: int y: int }"#,
        &[], &[]),
    ("gc_checks.sum_to", r#"function sum_to(n: int) -> int { let total = 0; let i = 0; while (i < n) { total += i; i += 1; } total }"#,
        &[], &[]),
    ("gc_checks.build", r#"function build(n: int) -> Point[] { let out: Point[] = []; let i = 0; while (i < n) { out.push(Point { x: i, y: i }); i += 1; } out }"#,
        &[], &[]),
    ("gc_checks.keep", r#"function keep(p: Point, n: int) -> int { let points = build(n); p.x + points.length() }"#,
        &[], &[]),
    ("gc_checks.main", r#"function main() -> int { sum_to(10) + keep(Point { x: 1, y: 2 }, 3) }"#,
        &[], &[]),
    ("generics.Point", r#"class Point { x int y int }"#,
        &[], &[]),
    ("generics.Box", r#"class Box<T> { value T function get(self) -> T { self.value } }"#,
        &[], &[]),
    ("generics.identity", r#"function identity<T>(x: T) -> T { x }"#,
        &[], &[]),
    ("generics.is_t", r#"function is_t<T>(witness: T, x: int | string) -> bool { x is T }"#,
        &[], &[]),
    ("generics.singleton", r#"function singleton<T>(x: T) -> T[] { [x] }"#,
        &[], &[]),
    ("generics.twice", r#"function twice<T>(x: T) -> T { identity<T>(identity<T>(x)) }"#,
        &[], &[]),
    ("generics.boxed", r#"function boxed<T>(x: T) -> Box<T> { Box { value: x } }"#,
        &[], &[]),
    ("generics.unbox", r#"function unbox<T>(b: Box<T>) -> T { b.get() }"#,
        &[], &[]),
    ("generics.is_box_int", r#"function is_box_int(x: Box<int> | Box<string>) -> bool { x is Box<int> }"#,
        &[], &[]),
    ("generics.id_int", r#"function id_int() -> (int) -> int throws never { identity<int> }"#,
        &[], &[]),
    ("generics.id_point", r#"function id_point() -> (Point) -> Point throws never { identity<Point> }"#,
        &[], &[]),
    ("generics.id_of", r#"function id_of<T>() -> (T) -> T throws never { identity<T> }"#,
        &[], &[]),
    ("generics.is_int_t", r#"function is_int_t<T>() -> bool { reflect.Type.of<T>() == reflect.Type.of<int>() }"#,
        &[], &[]),
    ("generics.bound", r#"function bound(t: reflect.Type) -> bool { type U = unreflect(t); reflect.Type.of<U>() == t }"#,
        &[], &[]),
    ("sibling_bindings.Wrapper", r#"class Wrapper<T> { inner T }"#,
        &[], &[]),
    ("sibling_bindings.sibling_bindings", r#"function sibling_bindings(a: reflect.Type, b: reflect.Type) -> reflect.Type[] { let first: reflect.Type = { let type T = a; reflect.Type.of<Wrapper<T>>() } let second: reflect.Type = { let type U = b; reflect.Type.of<Wrapper<U>>() } [first, second] }"#,
        &[], &[]),
    ("array_loop.total", r#"function total(xs: int[]) -> int { let total = 0; for (let v in xs) { total += v; } total }"#,
        &["vm.load_array_element(", "vm.container_len("], &["vm.virtual_target(", "ArrayIterator"]),
    ("catch.wildcard", r#"function wildcard(n: int) -> int { (1 / n) catch (e) { _ => -1 } }"#,
        &["vm.is_panic("], &["== -2i64"]),
    ("catch.all_panics", r#"function all_panics(n: int) -> int { (1 / n) catch_all_panics (e) { _ => -1 } }"#,
        &[], &["vm.is_panic("]),
    ("unwind.caught_loop", r#"function caught_loop(n: int) -> int { (1 / n) catch (e) { baml.panics.DivisionByZero => { let i = 0; while (i < n) { i += 1; } i } } }"#,
        &["vm.should_yield()", "vm.suspend(Step::Yield)"], &[]),
    ("recursive.left", r#"function left(s: string, n: int) -> string { if (n == 0) { s } else { right(s, n - 1) } }"#,
        &["vm.should_yield_at_call()", "vm.suspend(Step::Yield)", "vm.set_slot(fr,", "vm.slot(fr,"], &[]),
    ("recursive.right", r#"function right(s: string, n: int) -> string { if (n == 0) { s } else { left(s, n - 1) } }"#,
        &["vm.should_yield_at_call()", "vm.suspend(Step::Yield)", "vm.set_slot(fr,", "vm.slot(fr,"], &[]),
    ("recursive.caller", r#"function caller(s: string, n: int) -> string { left(s, n) + s }"#,
        &["vm.set_slot(fr,", "vm.slot(fr,"], &["vm.should_yield_at_call()"]),
    ("recursive.plain", r#"function plain(n: int) -> int { n + 1 }"#,
        &[], &["vm.should_yield_at_call()"]),
    ("trace.Reader", r#"interface Reader { function read(self, n: int) -> int throws never }"#,
        &[], &[]),
    ("trace.Probe", r#"class Probe { implements Reader { function read(self, n: int) -> int throws never { n } } }"#,
        &[], &[]),
    ("trace.traced", r#"function traced(p: Reader, xs: int[]) -> int { p.read(xs[0], $trace = trace.hidden()) }"#,
        &[], &[]),
    ("compute.uses_closure", "",
        &["vm.suspend("], &[]),
    ("<lambda(uses_closure, 0)>", "",
        &[], &["vm.suspend(", "vm.should_yield()"]),
];

#[test]
fn constructs_and_generated_shapes() {
    // Separate namespaces retain fixture isolation while sharing one compilation.
    let files: Vec<_> = CONSTRUCTS
        .iter()
        .enumerate()
        .filter(|(_, (_, source, _, _))| !source.is_empty())
        .map(|(i, (name, source, _, _))| {
            (
                format!("ns_{}/probe{i}.baml", name.split('.').next().unwrap()),
                *source,
            )
        })
        .collect();
    let sources: Vec<_> = files
        .iter()
        .map(|(name, source)| (name.as_str(), *source))
        .collect();
    let db = baml_test_support::setup_multi_file_db(&sources);
    let program = lower(&db, OptLevel::Two);
    let module = render(&db, &program, OptLevel::Two, None);
    for (object, function) in user_functions(&program) {
        assert!(
            module.functions.iter().any(|f| f.object == object),
            "{:?} should compile: {:?}",
            function.identity,
            module.rejected
        );
    }
    syn::parse_file(&module.source).expect("generated module parses as Rust");
    assert_admitted(
        &module,
        &[
            "baml.String.length",
            "baml.iter.Range.new",
            "baml.<(baml.iter.Range as baml.iter.Iterator)>.next",
            "baml.<(baml.iter.Range as baml.iter.Iterable)>.iter",
        ],
    );

    for (name, _, has, lacks) in CONSTRUCTS {
        if has.is_empty() && lacks.is_empty() {
            continue;
        }
        let body = body_of(&module, &format!("user.{name}"));
        for marker in *has {
            assert!(body.contains(marker), "{name}: missing {marker}: {body}");
        }
        for marker in *lacks {
            assert!(
                !body.contains(marker),
                "{name}: unexpected {marker}: {body}"
            );
        }
    }
    #[rustfmt::skip]
    let orderings = [
        ("sibling_bindings.sibling_bindings", "vm.type_value(", "vm.bind_type("),
        ("trace.traced", "vm.load_array_element(", "vm.set_call_trace("),
        ("trace.traced", "vm.virtual_target(", "vm.set_call_trace("),
        ("trace.traced", "vm.push_virtual_args(", "vm.set_call_trace("),
        ("trace.traced", "vm.set_call_trace(", "vm.suspend(Step::Call"),
    ];
    for (name, before, after) in orderings {
        let body = body_of(&module, &format!("user.{name}"));
        assert!(
            body.find(before).expect(before) < body.rfind(after).expect(after),
            "{name}: {body}"
        );
    }
    let lambda = module
        .functions
        .iter()
        .find(|f| f.display == "user.<lambda(cap_local, 0)>")
        .unwrap();
    assert_eq!(lambda.arity, 1);
}

#[test]
fn functions_whose_names_render_alike_compile_to_their_own_objects() {
    let db = baml_test_support::setup_multi_file_db(&[
        (
            "foo.baml",
            "class Foo {\n  x int\n  function m(self) -> int { self.x + 1 }\n}\n",
        ),
        ("ns_Foo/m.baml", "function m(x: int) -> int { x + 2 }\n"),
    ]);
    let program = lower(&db, OptLevel::Two);
    let module = render(&db, &program, OptLevel::Two, None);

    let alike: Vec<_> = module
        .functions
        .iter()
        .filter(|f| f.name == "user.Foo.m")
        .collect();
    assert_eq!(alike.len(), 2, "{:?}", module.rejected);
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

#[test]
fn entry_points_prune_admission_to_what_they_reach() {
    let db = baml_test_support::setup_test_db(FIXTURE);
    let program = lower(&db, OptLevel::Two);
    let callables = program.rendered_callables();
    #[rustfmt::skip]
    let cases: &[(&str, &[&str], &[&str])] = &[
        ("fib", &["fib"], &["sum", "main", "uses_closure"]),
        ("main", &["main", "sum", "fib", "manhattan", "safe_div", "mean"], &["build"]),
    ];
    for (entry, admitted, unreached) in cases {
        let entries = [callables[&format!("user.{entry}")].object.raw()];
        let module = render(&db, &program, OptLevel::Two, Some(&entries));
        for name in *admitted {
            assert_admitted(&module, &[&format!("user.{name}")]);
        }
        for name in *unreached {
            assert!(matches!(
                rejection(&module, &format!("user.{name}")),
                Rejection::Unreached
            ));
        }
        assert_eq!(
            module.functions.iter().any(|f| f.name.starts_with("baml.")),
            *entry == "main"
        );
        if *entry == "fib" {
            assert_eq!(module.functions.len(), 1);
            assert!(module.rejected.iter().any(
                |(name, why)| !name.starts_with("user.") && matches!(why, Rejection::Unreached)
            ));
        }
    }
}

#[test]
fn unsupported_programs_remain_bytecode() {
    let mut large = String::from("function large(n: int) -> int { let total = 0;\n");
    for i in 0..600 {
        large.push_str(&format!("if (n > {i}) {{ total += 1; }}\n"));
    }
    large.push_str("total }");
    #[rustfmt::skip]
    let cases = [
        ("answer", "function answer() -> int { 42 }", "without MIR positions"),
        ("<init>", "const VALUE = 42; function main() -> int { VALUE }", "initializer bytecode"),
        ("large", large.as_str(), "MIR blocks"),
    ];
    for (name, source, reason) in cases {
        let db = baml_test_support::setup_test_db(source);
        let mut program = lower(&db, OptLevel::Two);
        let object = if name == "<init>" {
            program.packages[program.root as usize].init.unwrap()
        } else {
            program.rendered_callables()[&format!("user.{name}")].object
        };
        match name {
            "answer" => {
                assert_admitted(
                    &render(&db, &program, OptLevel::Two, None),
                    &["user.answer"],
                );
                program = borsh::from_slice(&borsh::to_vec(&program).unwrap()).unwrap();
            }
            "<init>" => {
                let Object::Function(function) = &mut program.objects[object] else {
                    panic!("initializer")
                };
                function.bytecode.instructions[1] = Instruction::Pop(1);
            }
            _ => {}
        }
        let module = render(&db, &program, OptLevel::Two, None);
        assert!(!module.functions.iter().any(|f| f.object == object.raw()));
        assert!(
            matches!(rejection(&module, &program.function_name(object).unwrap()),
            Rejection::Unsupported(why) if why.contains(reason)),
            "{name}: {:?}",
            module.rejected
        );
        if name == "answer" {
            assert!(module.functions.is_empty());
        }
    }
}

#[test]
fn same_line_errors_keep_exact_bytecode_positions_on_error_branches() {
    let db = baml_test_support::setup_test_db(
        r#"
function Throw() -> int throws int { let n = 1; throw n }
function Caught() -> int { { let n = 1; throw n } catch (e) { int => 0 } }
function Boundary() -> int { let map = { "present": 1 }; map["missing"] }
"#,
    );
    for opt in [OptLevel::Zero, OptLevel::One, OptLevel::Two] {
        let program = lower(&db, opt);
        let module = render(&db, &program, opt, None);
        for name in ["user.Throw", "user.Caught", "user.Boundary"] {
            let object = program.rendered_callables()[name].object;
            let Object::Function(function) = &program.objects[object] else {
                unreachable!("a rendered callable is a function")
            };
            let pc = function
                .bytecode
                .instructions
                .iter()
                .rposition(|instruction| {
                    if name == "user.Boundary" {
                        matches!(
                            instruction,
                            Instruction::Call { .. } | Instruction::CallLocal { .. }
                        )
                    } else {
                        matches!(instruction, Instruction::Throw)
                    }
                })
                .unwrap();
            let body: String = body_of(&module, name).split_whitespace().collect();
            if name == "user.Boundary" {
                let expected = format!("Err(e)=>{{pc={pc};returnErr(e);}}");
                assert!(body.contains(&expected), "{name} at {opt:?}: {body}");
            } else {
                let marker = if name == "user.Throw" {
                    "returnErr(bex_vm::compiled_rt::throw("
                } else {
                    "fr.note_pc(pc);"
                };
                // Operand temporaries may separate the PC assignment and raise.
                let before = body.split_once(marker).unwrap().0;
                let assigned = before
                    .rsplit_once("pc=")
                    .unwrap()
                    .1
                    .split(';')
                    .next()
                    .unwrap();
                assert_eq!(assigned, pc.to_string(), "{name} at {opt:?}: {body}");
            }
        }
    }
}
