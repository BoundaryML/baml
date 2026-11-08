//! Compiler admission, generated Rust structure, and bytecode identity checks.

use baml_compiler2_emit::OptLevel;
use baml_db::rust_backend::{Rejection, RustModule, compile_program};
use bex_vm_types::{
    Instruction, Object, ObjectIndex,
    types::{Function, FunctionIdentity, Program},
};

fn lower(db: &baml_db::ProjectDatabase, opt: OptLevel) -> Program {
    baml_db::CheckedWorld::collect(db, db.workspace_root().unwrap())
        .expect("the world checks")
        .compile(opt)
        .unwrap()
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
fn unrelated_iteration_interfaces_are_compiled_with_virtual_dispatch() {
    let db = baml_test_support::setup_multi_file_db(&[(
        "iteration_identity.baml",
        include_str!("../rust_diff/baml_src/iteration_identity.baml"),
    )]);
    for opt in [OptLevel::Zero, OptLevel::One, OptLevel::Two] {
        let program = lower(&db, opt);
        let module = render(&db, &program, opt, None);
        let (object, _) = user_functions(&program)
            .find(|(_, function)| function.name() == "IteratorInterfaceIdentity")
            .expect("probe function");
        assert!(
            module.functions.iter().any(|f| f.object == object),
            "the differential probe must exercise native code at {opt:?}: {:?}",
            module.rejected
        );
    }
}

#[test]
fn initializers_and_global_calls_at_every_opt_level() {
    let db = baml_test_support::setup_multi_file_db(&[
        (
            "globals.baml",
            r#"
const FORWARD = LATER + 1;
const LATER = 41;
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
function main() -> string { let p = Point { x: 1, y: 2 }; let q = Point { x: 4, y: 6 }; `${sum([1, 2, 3])}|${fib(10)}|${manhattan(p, q)}|${safe_div(7, 0)}|${mean([1.0, 3.0])}|${p}` }
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
    ("generics.bound", r#"function bound(t: reflect.Type) -> bool { let type U = t; reflect.Type.of<U>() == t }"#,
        &[], &[]),
    ("sibling_bindings.Wrapper", r#"class Wrapper<T> { inner T }"#,
        &[], &[]),
    ("sibling_bindings.sibling_bindings", r#"function sibling_bindings(a: reflect.Type, b: reflect.Type) -> reflect.Type[] { let first: reflect.Type = { let type T = a; reflect.Type.of<Wrapper<T>>() }; let second: reflect.Type = { let type U = b; reflect.Type.of<Wrapper<U>>() }; [first, second] }"#,
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

#[test]
fn rt_trees_and_fail_closed_admission() {
    fn emit(source: &str) -> Result<String, String> {
        let db = baml_test_support::setup_multi_file_db(&[("rt.baml", source)]);
        let program = lower(&db, OptLevel::Two);
        let (entry, _) = user_functions(&program)
            .find(|(_, function)| function.name() == "main")
            .unwrap();
        baml_db::rust_backend::compile_rt(
            &db,
            db.workspace_root().unwrap(),
            &program,
            OptLevel::Two,
            entry,
        )
    }
    assert!(emit("/// baml:$trace=trace.hidden\nfunction main() -> int { 1 }").is_ok());
    for hook in ["trace.timing", "trace.span", "trace.empty_span"] {
        let source = format!("/// baml:$trace={hook}\nfunction main() -> int {{ 1 }}");
        assert!(emit(&source).unwrap().contains("hook_mode:Some("));
    }
    let trees = emit(include_str!(
        "../../../tools/speedtest/workloads/memory/binary-trees-14/main.baml"
    ))
    .unwrap();
    assert!(trees.contains("size:24,ptr_slots:(8,2)"));
    assert!(trees.contains("rt::telemetry::enter_observed("));
    assert!(trees.contains("rt::shl("));
    assert!(trees.contains("rt::Source"));
    assert!(trees.contains("baml_rt_gc::write_ptr("));
    assert!(!trees.contains("BexVm"));
    let richards = emit(include_str!(
        "../../../tools/speedtest/workloads/programs/richards-50/main.baml"
    ))
    .unwrap();
    assert!(richards.contains("static VTABLE"));
    assert!(richards.contains("rt::write_dyn("));
    assert!(richards.contains("rt::list_slot("));
    assert!(richards.contains("ElementLayout::Pointer"));
    for source in ["function main() -> int { let a = [[1], [2]]; a[0][0] }"] {
        assert!(
            emit(source).unwrap_err().contains("unsupported in baml_rt"),
            "{source}"
        );
    }
}

#[test]
fn allocation_failure_constructor_matches_interpreter_exception() {
    use std::sync::{Arc, atomic::AtomicBool};
    let program = baml_test_support::compile_source("function main() -> int { 0 }");
    let mut vm = bex_vm::BexVm::from_program(program, Arc::new(AtomicBool::new(false))).unwrap();
    let value = vm.panic_to_exception_value(bex_vm_types::errors::VmPanic::allocation_failure(16));
    let Object::Instance(instance) = vm.get_object(value.as_object_ptr().unwrap()) else {
        panic!("exception instance")
    };
    let Object::Class(class) = vm.get_object(instance.class) else {
        panic!("exception class")
    };
    let field = instance.try_load_field(0).unwrap();
    let Object::String(message) = vm.get_object(field.as_object_ptr().unwrap()) else {
        panic!("message field")
    };
    // Paired with the rt runtime's forced-limit test.
    assert_eq!(format!("baml.{}", class.path), "baml.panics.AllocFailure");
    assert_eq!(message.as_str(), "Allocation of 16 bytes failed");
}

#[test]
fn rt_string_receiver_storage_and_function_values() {
    for (source, accepted) in [
        (
            include_str!("../../../tools/speedtest/workloads/programs/agent-loop-300/main.baml"),
            true,
        ),
        (
            "function length(s: string) -> int { s.length() }\nfunction main() -> int { let f = length; f(\"x\") }",
            true,
        ),
        ("function main() -> int { \"x\".trim().length() }", false),
    ] {
        let db = baml_test_support::setup_multi_file_db(&[("strings.baml", source)]);
        let program = lower(&db, OptLevel::Two);
        let before = fingerprint(&program);
        let (entry, _) = user_functions(&program)
            .find(|(_, function)| function.name() == "main")
            .unwrap();
        let result = baml_db::rust_backend::compile_rt(
            &db,
            db.workspace_root().unwrap(),
            &program,
            OptLevel::Two,
            entry,
        );
        if accepted {
            let source = result.unwrap();
            assert!(source.contains("fn rt_string_char_count("));
            syn::parse_file(&source).expect("receiver and function value Rust");
        } else {
            assert!(result.unwrap_err().contains("unsupported in baml_rt"));
        }
        assert_eq!(before, fingerprint(&program));
    }
}

#[test]
fn rt_growing_lists_and_fail_closed_mutation() {
    for (source, accepted) in [
        (
            include_str!("../../../tools/speedtest/workloads/programs/agent-loop-300/main.baml"),
            true,
        ),
        (
            "function main() -> int { let xs: string[] = [\"a\"]; xs[0] = \"b\"; xs.length() }",
            false,
        ),
    ] {
        let db = baml_test_support::setup_multi_file_db(&[("lists.baml", source)]);
        let program = lower(&db, OptLevel::Two);
        let before = fingerprint(&program);
        let (entry, _) = user_functions(&program)
            .find(|(_, f)| f.name() == "main")
            .unwrap();
        let result = baml_db::rust_backend::compile_rt(
            &db,
            db.workspace_root().unwrap(),
            &program,
            OptLevel::Two,
            entry,
        );
        if accepted {
            let source = result.unwrap();
            assert!(source.contains("rt::lists::buffer_descriptor::<rt::strings::Str>"));
            assert!(source.contains("fn rt_array_push"));
        } else {
            assert!(result.unwrap_err().contains("unsupported in baml_rt"));
        }
        assert_eq!(before, fingerprint(&program));
    }
}

#[test]
fn rt_agent_maps_iteration_catch_and_json_admission() {
    for (source, accepted) in [
        (
            include_str!("../../../tools/speedtest/workloads/programs/agent-loop-300/main.baml"),
            true,
        ),
        (
            r#"class A { x: int } class B { y: int } function main() -> int { let a = baml.json.decode<A | B>("x") catch(e) { baml.json.ParseError => null }; 0 }"#,
            true,
        ),
        (
            r#"function main()->string { int.parse("bad").to_string() catch(e) { baml.errors.ParseError {message} => message } }"#,
            true,
        ),
        (
            "function main() -> int { let m: map<int,int> = {}; m[1] = 2; 0 }",
            true,
        ),
        (
            "function main() -> int { let m: map<string,string> = {}; m[\"a\"] = \"b\"; 0 }",
            true,
        ),
    ] {
        let db = baml_test_support::setup_multi_file_db(&[("agent.baml", source)]);
        let program = lower(&db, OptLevel::Two);
        let before = fingerprint(&program);
        let (entry, _) = user_functions(&program)
            .find(|(_, f)| f.name() == "main")
            .unwrap();
        let result = baml_db::rust_backend::compile_rt(
            &db,
            db.workspace_root().unwrap(),
            &program,
            OptLevel::Two,
            entry,
        );
        if accepted {
            result.unwrap();
        } else {
            assert!(result.unwrap_err().contains("unsupported in baml_rt"));
        }
        assert_eq!(before, fingerprint(&program));
    }
}

#[test]
fn rt_stack_placement_preserves_lifetimes_and_bytecode() {
    for (source, sites) in [
        (
            include_str!("../../../tools/speedtest/workloads/classes/method-call-100k/main.baml"),
            1,
        ),
        (
            "class C { x: int } function make() -> C { C { x: 1 } } function main() -> int { make().x }",
            0,
        ),
        (
            "class C { x: int } function alias(c: C) -> C { c } function main() -> int { let c = C { x: 1 }; alias(c).x }",
            1,
        ),
        (
            "class C { x: int } function main() -> int { let p = C { x: 0 }; let sum = 0; for (let i = 1; i < 9; i += 1) { let c = C { x: i }; sum += p.x; p = c; }; sum + p.x }",
            0,
        ),
        (
            include_str!("../rust_diff/baml_src/stack_allocation.baml"),
            4,
        ),
    ] {
        let entry_name = if source.contains("function StackAllocation()") {
            "StackAllocation"
        } else {
            "main"
        };
        let db = baml_test_support::setup_multi_file_db(&[("stack.baml", source)]);
        for opt in [OptLevel::Zero, OptLevel::One, OptLevel::Two] {
            let program = lower(&db, opt);
            let before = fingerprint(&program);
            let (entry, _) = user_functions(&program)
                .find(|(_, f)| f.name() == entry_name)
                .unwrap();
            let generated = baml_db::rust_backend::compile_rt(
                &db,
                db.workspace_root().unwrap(),
                &program,
                opt,
                entry,
            )
            .unwrap();
            syn::parse_file(&generated).expect("stack layouts parse");
            let actual = generated.matches("(&raw mut stack").count();
            assert!(
                actual <= sites,
                "unexpected stack site: {entry_name} at {opt:?}"
            );
            // Unoptimized trace hooks can conservatively retain arguments.
            if opt == OptLevel::Two {
                assert_eq!(actual, sites, "{entry_name} at {opt:?}");
                if source.contains("function norm2(self)") {
                    assert!(generated.contains("let stack_mode"));
                    assert!(generated.contains("=if OBSERVE {rt::telemetry::mode("));
                    assert!(generated.contains("==0 {f"));
                    assert!(generated.contains("_body::<OBSERVE>(cx,"));
                }
            }
            assert_eq!(before, fingerprint(&program));
            assert_eq!(
                before,
                fingerprint(&lower(&db, opt)),
                "escape queries must not change emitted bytecode"
            );
        }
    }
}

#[test]
// Byte hashes captured from the unsplit rt emitter. Intentional future
// codegen changes may update these after checking the generated-source diff.
fn rt_source_bytes_and_diagnostics() {
    use sha2::{Digest, Sha256};
    let mut changed = Vec::new();
    for (name, source) in [
        (
            "trees",
            include_str!("../../../tools/speedtest/workloads/memory/binary-trees-14/main.baml"),
        ),
        (
            "richards",
            include_str!("../../../tools/speedtest/workloads/programs/richards-50/main.baml"),
        ),
        (
            "agent",
            include_str!("../../../tools/speedtest/workloads/programs/agent-loop-300/main.baml"),
        ),
        ("initializer", "const X = 1; function main() -> int { X }"),
        ("entry", "function main() -> string { \"hello\" }"),
        (
            "nested_list",
            "function main() -> int { let a = [[1], [2]]; a[0][0] }",
        ),
        (
            "hook",
            "/// baml:$trace=trace.span\nfunction main() -> int { 1 }",
        ),
    ] {
        let db = baml_test_support::setup_multi_file_db(&[("rt.baml", source)]);
        let program = lower(&db, OptLevel::Two);
        let original = fingerprint(&program);
        let (entry, _) = user_functions(&program)
            .find(|(_, f)| f.name() == "main")
            .unwrap();
        let result = baml_db::rust_backend::compile_rt(
            &db,
            db.workspace_root().unwrap(),
            &program,
            OptLevel::Two,
            entry,
        );
        assert_eq!(
            fingerprint(&program),
            original,
            "{name}: rt changed bytecode"
        );
        let expected = match name {
            "trees" => "8999c3b17aac1c4ce8c2ef2e70e4bb436989f83d7282614362de04092468e8c8",
            "richards" => "00c12730e8c60763ec07deb208153e7b09928c1e050e048587abd18f6a797660",
            "agent" => "83c70cfcdeed67c65df4bceba16d0626783fa2c9d543df91b26751fd2a8f7401",
            _ => "",
        };
        if name == "initializer" {
            let source = result.unwrap();
            assert!(source.contains("rt::globals::begin("));
            assert!(source.contains("rt::globals::load"));
            continue;
        }
        if name == "hook" {
            assert!(result.unwrap().contains("InvocationMode::Span"));
            continue;
        }
        if name == "entry" {
            let generated = result.unwrap();
            assert!(generated.contains("rt::host::driver"));
            assert!(generated.contains(".copy_out("));
            continue;
        }
        if expected.is_empty() {
            let expected = match name {
                "entry" => "unsupported in baml_rt: entry must have no parameters and return int",
                "nested_list" => "unsupported in baml_rt: list element",
                "hook" => "unsupported in baml_rt: declaration hooks",
                _ => unreachable!(),
            };
            let actual = result.unwrap_err();
            if actual != expected {
                eprintln!("{name}: {actual:?} != {expected:?}");
                changed.push(name);
            }
        } else {
            let source = result.unwrap();
            if let Some(dir) = std::env::var_os("BAML_RT_SOURCE_DUMP") {
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(
                    std::path::Path::new(&dir).join(format!("{name}.rs")),
                    &source,
                )
                .unwrap();
            }
            let actual = format!("{:x}", Sha256::digest(source.as_bytes()));
            if actual != expected {
                eprintln!("{name}: {actual}");
                changed.push(name);
            }
        }
    }
    assert!(changed.is_empty(), "generated sources changed: {changed:?}");
}

#[test]
fn rt_host_scalars_io_and_statement_log_sites() {
    for source in [
        include_str!("../../../tools/speedtest/workloads/compute/nbody-20k/main.baml"),
        r#"class Reply { text:string, ok:bool, count:int, score:float } function main(value:Reply)->Reply { value }"#,
        r#"function main(n:int, label:string, yes:bool, x:float) -> string { if(yes) { `${label}:${n}:${x}` } else { label } }"#,
        r#"class Body { mass:float } function main()->int { let b=Body{mass:4.0}; b.mass.sqrt().itrunc()+int.parse("3") }"#,
        r#"function main()->void { log.info("one"); log.warn("two"); baml.io.println("logged"); }"#,
        r#"function main()->string? { baml.env.get("BAML_RT_MISSING_TEST_VARIABLE") }"#,
    ] {
        let db = baml_test_support::setup_multi_file_db(&[("rt_host.baml", source)]);
        let program = lower(&db, OptLevel::Two);
        let (entry, _) = user_functions(&program)
            .find(|(_, f)| f.name() == "main")
            .unwrap();
        let source = baml_db::rust_backend::compile_rt(
            &db,
            db.workspace_root().unwrap(),
            &program,
            OptLevel::Two,
            entry,
        )
        .unwrap();
        assert!(!source.contains("BexVm"));
        if source.contains("rt::telemetry::log(") {
            let sites = source
                .lines()
                .filter(|l| l.contains("rt::telemetry::log("))
                .collect::<Vec<_>>();
            assert_eq!(sites.len(), 2);
            assert_ne!(sites[0].split("u32").next(), sites[1].split("u32").next());
        }
    }
}

#[test]
fn rt_owned_native_admission_preserves_bytecode() {
    let db = baml_test_support::setup_multi_file_db(&[(
        "native.baml",
        include_str!("../rt_fixtures/native/baml_src/main.baml"),
    )]);
    let program = lower(&db, OptLevel::Two);
    let before = fingerprint(&program);
    for name in ["RtNativeFiles", "RtNativeProcess", "RtNativeCalendar"] {
        let (entry, _) = user_functions(&program)
            .find(|(_, f)| f.name() == name)
            .unwrap();
        baml_db::rust_backend::compile_rt(
            &db,
            db.workspace_root().unwrap(),
            &program,
            OptLevel::Two,
            entry,
        )
        .unwrap_or_else(|error| panic!("{name}: {error}"));
    }
    assert_eq!(fingerprint(&program), before);
}

#[test]
fn rt_generics_switch_aliases() {
    let source = include_str!("../rust_diff/type_dispatch.baml");
    let db = baml_test_support::setup_multi_file_db(&[("rt.baml", source)]);
    let program = lower(&db, OptLevel::Two);
    let (entry, _) = user_functions(&program)
        .find(|(_, f)| f.name() == "main")
        .unwrap();
    let source = baml_db::rust_backend::compile_rt(
        &db,
        db.workspace_root().unwrap(),
        &program,
        OptLevel::Two,
        entry,
    )
    .unwrap();
    std::fs::write(
        std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("rt-type-program.rs"),
        source,
    )
    .unwrap();
}

#[test]
fn rt_shared_recursive_forwarded_and_stored_closures() {
    let source = include_str!("../rt_fixtures/closures/baml_src/main.baml");
    let db = baml_test_support::setup_multi_file_db(&[("closures.baml", source)]);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    for opt in [OptLevel::Zero, OptLevel::One, OptLevel::Two] {
        let program = lower(&db, opt);
        let entry = program.rendered_callables()["user.main"].object.raw();
        let emitted = baml_db::rust_backend::compile_rt(
            &db,
            db.workspace_root().unwrap(),
            &program,
            opt,
            entry,
        )
        .unwrap();
        let empty = program.rendered_callables()["user.empty"].object.raw();
        let empty_source = baml_db::rust_backend::compile_rt(
            &db,
            db.workspace_root().unwrap(),
            &program,
            opt,
            empty,
        )
        .unwrap();
        syn::parse_file(&empty_source).expect("empty callable list Rust");
        assert!(empty_source.contains("rt::closures::closure_descriptor("));
        if std::env::var_os("BAML_RT_DUMP_SOURCE").is_some() {
            std::fs::write(format!("/tmp/rt-empty-closures-{opt:?}.rs"), &empty_source).unwrap();
        }
        syn::parse_file(&emitted).expect("closures emit valid Rust");
        assert!(emitted.contains("rt::closures::capture(env,"));
        assert!(emitted.contains("rt::closures::store("));
        assert!(!emitted.contains("BexVm"));
        if std::env::var_os("BAML_RT_DUMP_SOURCE").is_some() {
            std::fs::write(format!("/tmp/rt-closures-{opt:?}.rs"), &emitted).unwrap();
        }
        assert_eq!(
            runtime
                .block_on(baml_tests::engine::try_call_by_name(program, "user.main"))
                .unwrap(),
            bex_engine::BexExternalValue::Int(38),
        );
    }
}

#[test]
fn rt_initializer_dependency_order_and_generic_maps() {
    for source in [
        "function main()->int { let m:map<string?,int> = {}; m[null]=1; m[\"key\"]=2; m[null]+m[\"key\"] }",
        "const SECOND = FIRST + 1; const FIRST = 41; function main() -> int { SECOND }",
        "const M: map<string,int> = {\"a\":1,\"b\":2}; function main() -> int { M.set(\"a\",3); M[\"a\"] + M.length() }",
        "class Box { m: map<int,string> } function main() -> int { let b=Box{m:{1:\"a\",2:\"b\"}}; b.m[1]=\"c\"; b.m.delete(2); b.m.length() }",
        "function main() -> int { let m: map<string,int?> = {\"null\":null,\"some\":7}; if(m.has(\"null\")){m.get(\"some\") ?? 0}else{99} }",
    ] {
        let db = baml_test_support::setup_multi_file_db(&[("maps_globals.baml", source)]);
        for opt in [OptLevel::Zero, OptLevel::One, OptLevel::Two] {
            let program = lower(&db, opt);
            let (entry, _) = user_functions(&program)
                .find(|(_, f)| f.name() == "main")
                .unwrap();
            let emitted = baml_db::rust_backend::compile_rt(
                &db,
                db.workspace_root().unwrap(),
                &program,
                opt,
                entry,
            )
            .unwrap_or_else(|e| panic!("{source} at {opt:?}: {e}"));
            assert!(!emitted.contains("BexVm"));
            syn::parse_file(&emitted).unwrap_or_else(|e| panic!("{source}: {e}"));
        }
    }
}

#[test]
fn rt_structural_union_map_keys_fail_closed() {
    for (source, expected) in [
        (
            "class Key { x: int } function main()->int { let m:map<Key?,int> = {}; m[null]=1; m.length() }",
            "map key callbacks",
        ),
        (
            "class Box { x:int } function main()->int {let m:map<string,Box?> = {\"a\":Box{x:1},\"b\":null}; m.values().length()}",
            "map snapshot declared list element type",
        ),
    ] {
        let db = baml_test_support::setup_multi_file_db(&[("admission.baml", source)]);
        let program = lower(&db, OptLevel::Two);
        let entry = program.rendered_callables()["user.main"].object.raw();
        let result = baml_db::rust_backend::compile_rt(
            &db,
            db.workspace_root().unwrap(),
            &program,
            OptLevel::Two,
            entry,
        );
        assert_eq!(
            result.unwrap_err(),
            format!("unsupported in baml_rt: {expected}")
        );
    }
}
