//! The native Rust backend (`baml_compiler2_rust`) from BAML source: what
//! compiles, what the output looks like, and what is rejected. The backend's
//! structurizer, type-mapping and must-initialization analyses have unit
//! tests in-crate; these tests go through the compiler on real programs.
//!
//! Everything here is compile-level: the generated Rust is checked to be
//! well-formed (the backend parses it back) and inspected as text; it is not
//! built or run. `tests/native_rust.rs` builds and runs fixtures against the
//! VM.

use baml_db::{
    ProjectDatabase,
    baml_compiler2_hir::{
        item_data::{file_functions, function_data},
        loc::FunctionLoc,
    },
    baml_compiler2_rust::{
        self as native, NativeModule, NativeTy, Rejection, admit, compile, compile_many, rust_name,
        rust_name_for,
    },
};
use baml_test_support::{assert_no_user_diagnostic_errors, setup_multi_file_db, setup_test_db};

fn function_named<'db>(db: &'db ProjectDatabase, name: &str) -> FunctionLoc<'db> {
    db.workspace_files()
        .into_iter()
        .flat_map(|file| file_functions(db, file).clone())
        .find(|&loc| function_data(db, loc).name == name)
        .unwrap_or_else(|| panic!("test source declares `{name}`"))
}

/// Compile `entry` from `source`, asserting the output is well-formed Rust
/// without a block dispatcher.
fn compile_entry(source: &str, entry: &str) -> NativeModule<'static> {
    compile_roots(source, &[entry])
}

/// Compile several roots from `source` into one module.
fn compile_roots(source: &str, entries: &[&str]) -> NativeModule<'static> {
    let db = Box::leak(Box::new(setup_test_db(source)));
    assert_no_user_diagnostic_errors(db);
    let roots: Vec<_> = entries
        .iter()
        .map(|entry| function_named(db, entry))
        .collect();
    // The backend parses its own output with `syn` before returning it (a
    // failure is `Rejection::Invalid`), so `Ok` means well-formed Rust.
    let module = compile_many(db, &roots).unwrap_or_else(|rejection| panic!("{rejection}"));
    assert!(
        !module.rust_source.contains("loop {\n        match"),
        "no block dispatcher:\n{}",
        module.rust_source
    );
    module
}

fn reject(source: &str, entry: &str) -> Rejection {
    let db = setup_test_db(source);
    assert_no_user_diagnostic_errors(&db);
    let loc = function_named(&db, entry);
    let rejection = compile(&db, loc).expect_err("compile rejects");
    assert_eq!(
        admit(&db, loc)
            .err()
            .map(|r| matches!(r, Rejection::Unsupported(_))),
        Some(true),
        "admit agrees with compile for {entry}"
    );
    rejection
}

fn assert_unsupported(rejection: &Rejection, needle: &str) {
    match rejection {
        Rejection::Unsupported(reason) => {
            assert!(
                reason.contains(needle),
                "reason `{reason}` lacks `{needle}`"
            );
        }
        Rejection::Invalid(reason) => panic!("expected unsupported, got invalid: {reason}"),
    }
}

fn assert_contains(source: &str, needle: &str) {
    assert!(source.contains(needle), "missing `{needle}` in:\n{source}");
}

// ── Scalar subset (v1) ──────────────────────────────────────────────────────

#[test]
fn nested_loops_with_break_and_continue() {
    let module = compile_entry(
        r"
function nested(n: int) -> int {
    let total = 0;
    let i = 0;
    while (i < n) {
        i = i + 1;
        if (i % 2 == 0) { continue; }
        let j = 0;
        while (j < i) {
            j = j + 1;
            if (j == 5) { break; }
            total = total + j;
        }
        if (total > 1000) { break; }
    }
    total
}
",
        "nested",
    );
    let source = &module.rust_source;
    assert_contains(
        source,
        "pub fn user_nested(mut _1: Int63) -> Result<Int63, Thrown>",
    );
    assert_eq!(source.matches(": loop {").count(), 2, "{source}");
    assert_contains(source, "continue 'bb");
    assert_contains(source, "break 'bb");
    assert_contains(source, "int::add(");
    assert_contains(source, "int::rem(");
    assert_eq!(module.functions.len(), 1);
    assert_eq!(
        module.functions[0].params,
        vec![("n".to_string(), NativeTy::Int)]
    );
    assert_eq!(module.functions[0].ret, NativeTy::Int);
    assert!(module.mir_dump.contains("fn user.nested(n: int) -> int"));
}

#[test]
fn int_match_with_wildcard_and_early_return() {
    let module = compile_entry(
        r"
function classify(x: int) -> int {
    match (x) {
        0 => return -1,
        1 => 10,
        2 => 20,
        3 => 20,
        _ => x * 2,
    }
}
",
        "classify",
    );
    let source = &module.rust_source;
    assert_contains(source, "match _1.get() {");
    assert_contains(source, "0i64 =>");
    assert_contains(source, "_ =>");
    assert_contains(source, "int::neg(");
    assert!(
        source.contains("    Ok(_0)\n}"),
        "tail exit is the value: {source}"
    );
}

#[test]
fn bool_match_uses_literal_tests() {
    let module = compile_entry(
        r"
function pick(b: bool) -> int {
    match (b) {
        true => 1,
        false => 0,
    }
}
",
        "pick",
    );
    let source = &module.rust_source;
    // A `bool` literal test is the operand itself (or its negation), never
    // `== true` / `== false`.
    assert!(
        source.contains("= _1;") || source.contains("= !_1;"),
        "{source}"
    );
    assert!(
        !source.contains("== true") && !source.contains("== false"),
        "{source}"
    );
}

#[test]
fn short_circuit_operators() {
    let module = compile_entry(
        r"
function check(a: int, b: int) -> bool {
    (a > 0 && b > 0) || a == b
}
",
        "check",
    );
    let source = &module.rust_source;
    assert_contains(source, "-> Result<bool, Thrown>");
    assert_contains(source, "= false;");
    assert_contains(source, "= true;");
}

#[test]
fn early_return_inside_a_loop() {
    let module = compile_entry(
        r"
function find(n: int) -> int {
    let i = 0;
    while (true) {
        if (i * i > n) { return i; }
        i = i + 1;
    }
}
",
        "find",
    );
    let source = &module.rust_source;
    assert_contains(source, ": loop {");
    assert_contains(source, "    Ok(_0)\n}");
    // The loop's own exit edge assigns `null` to the `int` return place: the
    // checker's proof that the edge is dead, emitted as an unreachable panic.
    assert_contains(source, "return Err(Thrown::from(Panic::Unreachable));");
}

#[test]
fn direct_call_between_two_functions() {
    let module = compile_entry(
        r"
function square(x: int) -> int { x * x }
function sum_of_squares(n: int) -> int {
    let total = 0;
    let i = 1;
    while (i <= n) {
        total = total + square(i);
        i = i + 1;
    }
    total
}
",
        "sum_of_squares",
    );
    let source = &module.rust_source;
    assert_contains(source, "= user_square(_");
    assert_contains(source, ")?;");
    let names: Vec<&str> = module
        .functions
        .iter()
        .map(|f| f.rust_name.as_str())
        .collect();
    assert_eq!(
        names,
        ["user_square", "user_sum_of_squares"],
        "callees first"
    );
    assert_eq!(module.entry, 1);
    assert_eq!(module.roots, vec![1]);
    assert_eq!(module.functions[0].link_name, "user.square");
}

#[test]
fn panic_with_a_constant_message() {
    let module = compile_entry(
        r#"
function guarded(n: int) -> int {
    if (n < 0) { baml.sys.panic("negative input"); }
    n
}
"#,
        "guarded",
    );
    let source = &module.rust_source;
    assert_contains(source, "Panic::UserPanic {");
    assert_contains(source, "String::from(\"negative input\")");
    assert_eq!(module.functions.len(), 1, "panic is not a callee");
}

#[test]
fn void_function() {
    let module = compile_entry(
        r"
function noop(n: int) -> void {
    let x = n + 1;
}
function run(n: int) -> int {
    noop(n);
    n
}
",
        "run",
    );
    let source = &module.rust_source;
    assert_contains(
        source,
        "pub fn user_noop(mut _1: Int63) -> Result<(), Thrown>",
    );
    assert_contains(source, "= user_noop(_1)?;");
    assert_eq!(module.functions[0].ret, NativeTy::Null);
}

#[test]
fn int_literals_fold_at_compile_time() {
    let module = compile_entry("function k() -> int { 42 }", "k");
    assert_contains(
        &module.rust_source,
        "const { Int63::new(42i64).expect(\"int literal fits int\") }",
    );
}

// ── Heap subset (v2) ────────────────────────────────────────────────────────

#[test]
fn strings_concat_compare_and_measure() {
    let module = compile_entry(
        r#"
function greet(name: string, shout: bool) -> string {
    let text = "hello, " + name;
    if (text == "hello, world") { text = text + "!"; }
    if (name < "m" && name.is_ascii()) { text = text + " (early)"; }
    if (name.length() > 3) { text = text + " (long)"; }
    text
}
"#,
        "greet",
    );
    let source = &module.rust_source;
    assert_contains(
        source,
        "pub fn user_greet(mut _1: Str, mut _2: bool) -> Result<Str, Thrown>",
    );
    assert_contains(source, "string::from_literal(\"hello, \")");
    assert_contains(source, "string::concat(&");
    assert_contains(source, "string::eq(&");
    assert_contains(source, "string::cmp(&");
    assert_contains(source, ".is_lt()");
    assert_contains(source, "string::is_ascii(&");
    assert_contains(source, "string::length(&");
    assert_eq!(
        module.functions[0].params,
        vec![
            ("name".to_string(), NativeTy::Str),
            ("shout".to_string(), NativeTy::Bool)
        ]
    );
    assert_eq!(module.functions[0].ret, NativeTy::Str);
}

#[test]
fn floats_use_native_arithmetic_and_total_order_comparisons() {
    let module = compile_entry(
        r"
function f(x: float, y: float) -> bool {
    let z = (x * 2.5 - y) / 0.5 + -x;
    z >= y && z != x
}
",
        "f",
    );
    let source = &module.rust_source;
    assert_contains(source, "mut _1: f64");
    assert_contains(source, "2.5_f64");
    assert_contains(source, "float::ge(");
    assert_contains(source, "!float::eq(");
    assert_contains(source, "= -_");
    assert!(
        !source.contains("int::"),
        "no checked int ops on floats:\n{source}"
    );
}

#[test]
fn arrays_literal_index_len_and_push() {
    let module = compile_entry(
        r"
function f(xs: int[]) -> int {
    let out: int[] = [1, 2];
    let i = 0;
    while (i < xs.length()) {
        out.push(xs[i]);
        out[0] = out[0] + xs[i];
        i += 1;
    }
    out[0] + out.length()
}
",
        "f",
    );
    let source = &module.rust_source;
    assert_contains(source, "mut _1: Shared<Vec<Int63>>");
    assert_contains(source, "array::new::<");
    assert_contains(source, "Vec::from([");
    assert_contains(source, "array::len(&");
    assert_contains(source, "array::get(&");
    assert_contains(source, "array::set(&");
    assert_contains(source, "array::push(&");
    // Heap locals are declared without a zero value; an empty literal names
    // its element type.
    assert_contains(source, "let mut _2: Shared<Vec<Int63>>;");
    let empty = compile_entry("function g() -> int[] { let xs: int[] = []; xs }", "g");
    assert_contains(&empty.rust_source, "array::new::<Int63>(Vec::new())");
    assert_eq!(
        module.functions[0].params,
        vec![("xs".to_string(), NativeTy::Array(Box::new(NativeTy::Int)))]
    );
}

#[test]
fn for_in_refines_the_iterator_and_element_locals() {
    let module = compile_entry(
        r"
function total(xs: int[]) -> int {
    let sum = 0;
    for (let x in xs) { sum += x; }
    sum
}
",
        "total",
    );
    let source = &module.rust_source;
    // The `Iterator<..>` temp and the `unknown` result of `next` take refined
    // types; the `Done` test is `is_none` and the element copy an `expect`.
    assert_contains(source, ": bex_lang::array::Iter<Int63>;");
    assert_contains(source, ": Option<Int63>;");
    assert_contains(source, "= array::iter(&_1);");
    assert_contains(source, "= array::next(&mut _");
    assert_contains(source, ".is_none();");
    // A `Copy` option is unwrapped in place, never cloned.
    assert_contains(source, ".expect(\"for-in element after Done check\")");
    assert!(!source.contains(".clone().expect"), "{source}");
}

#[test]
fn class_struct_with_nullable_field_and_aggregate() {
    let module = compile_entry(
        r#"
class Cell { value: int, label: string | null, payload: int[], type: float }
function make(n: int) -> int {
    let cell = Cell { value: n, payload: [n], type: 1.5 };
    let alias = cell;
    alias.value += 1;
    alias.payload[0] += 1;
    if (cell.label == null) { cell.label = "x"; }
    cell.value + cell.payload[0]
}
"#,
        "make",
    );
    let source = &module.rust_source;
    assert_contains(source, "pub struct user_Cell {");
    assert_contains(source, "pub value: Int63,");
    assert_contains(source, "pub label: Option<Str>,");
    assert_contains(source, "pub payload: Shared<Vec<Int63>>,");
    // A field named like a Rust keyword keeps its BAML name on the wire.
    assert_contains(source, "#[serde(rename = \"type\")]");
    assert_contains(
        source,
        "#[serde(serialize_with = \"bex_lang::json::serialize_f64\")]",
    );
    assert_contains(source, "pub type_: f64,");
    assert_contains(
        source,
        "#[serde(crate = \"bex_lang::serde\", expecting = \"expected JSON object for class `Cell`\")]",
    );
    assert_contains(source, "impl ToBaml for user_Cell {");
    assert_contains(source, "bex_lang::render::class(");
    assert_contains(source, "\"Cell\",");
    assert_contains(source, "(\"value\", &self.value as &dyn ToBaml)");
    // Construction: unspecified nullable slot is `None`.
    assert_contains(source, "shared(user_Cell {");
    assert_contains(source, "label: None");
    // Field reads borrow, field writes borrow mutably after computing the value.
    assert_contains(source, ".borrow().value");
    assert_contains(source, ".borrow_mut().value = value;");
    assert_contains(source, ".borrow_mut().label = value;");
    assert_contains(source, ".is_none()");
    assert_eq!(module.classes.len(), 1);
    assert_eq!(module.classes[0].link_name, "user.Cell");
    assert_eq!(module.classes[0].rust_name, "user_Cell");
}

#[test]
fn structs_expect_a_json_object_named_like_the_vm_does() {
    // A workspace class at the root displays bare; a class in a namespace
    // directory (`ns_geo/`) displays as `geo.Point`, the VM's decode-error
    // wording. The struct identifier stays the sanitized link name.
    let db = Box::leak(Box::new(setup_multi_file_db(&[
        (
            "main.baml",
            "class State { n: int }\nfunction prepare(raw: string) -> State { baml.json.deserialize<State>(raw) }",
        ),
        (
            "ns_geo/point.baml",
            "class Point { x: int }\nfunction load(raw: string) -> Point { baml.json.deserialize<Point>(raw) }",
        ),
    ])));
    assert_no_user_diagnostic_errors(db);
    let module = compile_many(
        db,
        &[function_named(db, "prepare"), function_named(db, "load")],
    )
    .unwrap_or_else(|rejection| panic!("{rejection}"));
    // prettyplease may break the attribute across lines; compare without
    // whitespace.
    let flat: String = module
        .rust_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    assert_contains(
        &flat,
        "#[serde(crate=\"bex_lang::serde\",expecting=\"expectedJSONobjectforclass`State`\")]pubstructuser_State{",
    );
    assert_contains(
        &flat,
        "#[serde(crate=\"bex_lang::serde\",expecting=\"expectedJSONobjectforclass`geo.Point`\")]pubstructuser_geo_Point{",
    );
    let source = &module.rust_source;
    assert_contains(source, "bex_lang::render::class(out, \"Point\", &[");
    assert_contains(source, "json::deserialize::<Shared<user_geo_Point>>(&_1)?");
}

#[test]
fn json_deserialize_is_monomorphized_on_the_load_type() {
    let module = compile_entry(
        r#"
class Inner { score: float }
class State { count: int, inner: Inner, tags: string[] }
function prepare(raw: string) -> State { baml.json.deserialize<State>(raw) }
"#,
        "prepare",
    );
    let source = &module.rust_source;
    assert_contains(source, "json::deserialize::<Shared<user_State>>(&_1)?");
    assert_contains(source, "pub struct user_State {");
    assert_contains(source, "pub inner: Shared<user_Inner>,");
    assert_contains(source, "pub struct user_Inner {");
    // The `load_type` temp produces no code.
    assert!(!source.contains("load_type"), "{source}");
    assert!(
        !source.contains("_2 ="),
        "type local is not assigned:\n{source}"
    );
    assert_eq!(
        module.functions[0].ret,
        NativeTy::Class(match &module.functions[0].ret {
            NativeTy::Class(class) => *class,
            other => panic!("return type is a class, got {other:?}"),
        })
    );
    let names: Vec<&str> = module
        .classes
        .iter()
        .map(|c| c.rust_name.as_str())
        .collect();
    assert_eq!(names, ["user_State", "user_Inner"], "first-seen order");
}

#[test]
fn to_string_and_json_to_string() {
    let module = compile_entry(
        r"
class State { values: int[] }
function run(state: State) -> string {
    let a = state.values.length().to_string();
    let b = baml.json.to_string(state);
    let c = baml.json.to_string([1, 2]);
    a + b + c
}
",
        "run",
    );
    let source = &module.rust_source;
    assert_contains(source, "= ToBaml::to_baml(&_");
    assert_contains(source, "= json::to_string(&_");
    assert_contains(source, "mut _1: Shared<user_State>");
}

#[test]
fn to_string_on_a_class_is_the_structural_rendering() {
    let module = compile_entry(
        r#"
class Point { x: int, y: int }
class Box { name: string, corner: Point, tag: string | null, weights: int[] }
function render(n: int) -> string {
    let p = Point { x: n, y: n + 1 };
    let b = Box { name: "b", corner: p, tag: null, weights: [n] };
    p.to_string() + b.to_string() + b.weights.to_string() + b.tag.to_string()
}
"#,
        "render",
    );
    let source = &module.rust_source;
    assert_eq!(
        source.matches("= ToBaml::to_baml(&_").count(),
        4,
        "{source}"
    );
    assert_contains(source, "impl ToBaml for user_Point {");
    assert_contains(source, "impl ToBaml for user_Box {");
}

#[test]
fn rejects_to_string_on_a_class_with_its_own_to_string() {
    let rejection = reject(
        r#"
class Named {
    name: string
    implements baml.ToString {
        function to_string(self) -> string { "Named(" + self.name + ")" }
    }
}
function f(x: Named) -> string { x.to_string() }
"#,
        "f",
    );
    assert_unsupported(&rejection, "to_string");
    assert_unsupported(&rejection, "user.Named");
}

#[test]
fn sort_on_a_primitive_array() {
    let module = compile_entry(
        r"
function f(xs: int[], names: string[]) -> int {
    xs.sort();
    names.sort();
    xs[0]
}
",
        "f",
    );
    let source = &module.rust_source;
    assert_contains(source, "array::sort_int(&_1);");
    assert_contains(source, "array::sort_str(&_2);");
}

#[test]
fn nullable_locals_and_null_comparison() {
    let module = compile_entry(
        r"
function f(x: int | null, y: int) -> int {
    let z: int | null = null;
    if (y > 0) { z = y; }
    if (x == null) { 0 } else if (z != null) { 1 } else { 2 }
}
",
        "f",
    );
    let source = &module.rust_source;
    assert_contains(source, "mut _1: Option<Int63>");
    assert_contains(source, "= None;");
    assert_contains(source, "_1.is_none()");
    assert_eq!(source.matches(".is_none()").count(), 2, "{source}");
    assert_eq!(
        module.functions[0].params[0].1,
        NativeTy::Option(Box::new(NativeTy::Int))
    );
}

#[test]
fn write_project_lays_out_a_crate() {
    let module = compile_entry(
        r#"
function main(n: int, verbose: bool, name: string, ratio: float) -> string {
    if (verbose) { name + (n * 2).to_string() } else { name + ratio.to_string() }
}
"#,
        "main",
    );
    let dir = std::env::temp_dir().join(format!(
        "baml_compiler2_rust_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    ));
    let options = native::ProjectOptions {
        crate_name: "native-main",
        runtime_path: std::path::Path::new("/tmp/runtime/bex_lang"),
        release_profile: true,
    };
    native::write_project(&module, &dir, &options).expect("write project");
    let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    assert_contains(&manifest, "name = \"native-main\"");
    assert_contains(&manifest, "[workspace]");
    assert_contains(&manifest, "bex_lang = { path = \"/tmp/runtime/bex_lang\" }");
    assert!(
        !manifest.contains("serde"),
        "serde comes through bex_lang:\n{manifest}"
    );
    assert_contains(&manifest, "lto = \"fat\"");
    assert_eq!(
        std::fs::read_to_string(dir.join("src/lib.rs")).unwrap(),
        module.rust_source
    );
    let shim = std::fs::read_to_string(dir.join("src/main.rs")).unwrap();
    assert_contains(&shim, "\"--n\" => arg_0 = Some(parse_int(\"n\", value)),");
    assert_contains(
        &shim,
        "\"--verbose\" => arg_1 = Some(parse_bool(\"verbose\", value)),",
    );
    assert_contains(
        &shim,
        "\"--name\" => arg_2 = Some(parse_string(\"name\", value)),",
    );
    assert_contains(
        &shim,
        "\"--ratio\" => arg_3 = Some(parse_float(\"ratio\", value)),",
    );
    assert_contains(
        &shim,
        "native_main::user_main(value_0, value_1, value_2, value_3)",
    );
    assert_contains(
        &shim,
        "let rendered = bex_lang::render::ToBaml::to_baml(&value);",
    );
    assert_contains(&shim, "uncaught throw");
    assert_contains(&shim, "thrown.exit_code()");
    assert_contains(
        &shim,
        "usage: native-main --n <int> --verbose <bool> --name <string> --ratio <float>",
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("mir.txt")).unwrap(),
        module.mir_dump
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn write_project_with_a_class_parameter_is_library_only() {
    let module = compile_entry(
        r"
class State { n: int }
function run(state: State) -> string { state.n.to_string() }
",
        "run",
    );
    let dir = std::env::temp_dir().join(format!(
        "baml_compiler2_rust_lib_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    ));
    let options = native::ProjectOptions {
        crate_name: "native-run",
        runtime_path: std::path::Path::new("/tmp/runtime/bex_lang"),
        release_profile: false,
    };
    native::write_project(&module, &dir, &options).expect("write project");
    assert!(!module.functions[module.entry].shim_callable());
    let shim = std::fs::read_to_string(dir.join("src/main.rs")).unwrap();
    assert_contains(
        &shim,
        "error: entry `user.run` takes non-scalar arguments; link the library instead",
    );
    assert_contains(&shim, "std::process::exit(2)");
    assert!(!shim.contains("parse_int"), "{shim}");
    assert!(
        std::fs::read_to_string(dir.join("src/lib.rs"))
            .unwrap()
            .contains("pub fn user_run("),
        "the library is still written"
    );
    let db = setup_test_db(
        "class State { n: int }\nfunction run(state: State) -> string { state.n.to_string() }\nfunction f(n: int) -> int { n }",
    );
    let run = admit(&db, function_named(&db, "run")).expect("admitted");
    assert!(!run.shim_callable());
    assert!(
        admit(&db, function_named(&db, "f"))
            .expect("admitted")
            .shim_callable()
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn compile_many_shares_callees_between_roots() {
    let source = r"
function square(x: int) -> int { x * x }
function a(n: int) -> int { square(n) + 1 }
function b(n: int) -> int { square(n) - 1 }
function unrelated(n: int) -> bool { n > 0 }
";
    let db = setup_test_db(source);
    assert_no_user_diagnostic_errors(&db);
    let roots = [
        function_named(&db, "b"),
        function_named(&db, "a"),
        function_named(&db, "unrelated"),
    ];
    let module = compile_many(&db, &roots).unwrap_or_else(|rejection| panic!("{rejection}"));
    let names: Vec<&str> = module
        .functions
        .iter()
        .map(|f| f.rust_name.as_str())
        .collect();
    assert_eq!(names, ["user_square", "user_b", "user_a", "user_unrelated"]);
    assert_eq!(module.roots, vec![1, 2, 3], "request order");
    assert_eq!(module.entry, 1, "the first root is the entry");
    assert_eq!(rust_name_for(&db, roots[0]), "user_b");
    assert_eq!(
        module.rust_source.matches("pub fn user_square(").count(),
        1,
        "a shared callee is compiled once"
    );
}

#[test]
fn rust_names_are_a_pure_function_of_the_link_name() {
    assert_eq!(rust_name("user.sum_of_squares"), "user_sum_of_squares");
    assert_eq!(rust_name("user.ns.f"), "user_ns_f");
    assert_eq!(rust_name("user.Cell"), "user_Cell");
    assert_eq!(rust_name("9lives"), "_9lives");
    assert_eq!(rust_name(""), "_");
}

// ── The ten synchronous benchmark cases ─────────────────────────────────────
//
// `instrumentation/cases/<case>/workload.baml`, inlined so the tests do not
// depend on the sibling repository. `prepare` and `run` of each must compile
// (with their callees) into one module.

const PREPARE: &str =
    "function prepare(raw: string) -> State { baml.json.deserialize<State>(raw) }\n";

fn compile_benchmark(source: &str) -> NativeModule<'static> {
    let module = compile_roots(source, &["prepare", "run"]);
    let source = &module.rust_source;
    assert_contains(
        source,
        "pub fn user_prepare(mut _1: Str) -> Result<Shared<user_State>, Thrown>",
    );
    assert_contains(
        source,
        "pub fn user_run(mut _1: Shared<user_State>) -> Result<Str, Thrown>",
    );
    assert_contains(source, "json::deserialize::<Shared<user_State>>(&_1)?");
    assert_contains(source, "pub struct user_State {");
    module
}

#[test]
fn bench_00_startup() {
    let module = compile_benchmark(&format!(
        "class State {{ value: int }}\n{PREPARE}\
         function run(state: State) -> string {{ state.value.to_string() }}"
    ));
    assert_contains(&module.rust_source, "= ToBaml::to_baml(&_2);");
}

#[test]
fn bench_01_array_traversal() {
    let module = compile_benchmark(&format!(
        r#"class State {{ values: int[], variant: string }}
{PREPARE}
function run(state: State) -> string {{
    let values = state.values;
    if (state.variant == "build") {{
        values = [];
        let j = 0;
        while (j < state.values.length()) {{
            values.push(state.values[j]);
            j += 1;
        }}
    }}
    let total = 0;
    if (state.variant == "indexed") {{
        let i = 0;
        while (i < values.length()) {{ total += values[i]; i += 1; }}
    }} else {{
        for (let value in values) {{ total += value; }}
    }}
    total.to_string()
}}"#
    ));
    let source = &module.rust_source;
    // The receiver temp of `.length()` is declared with a function type by
    // lowering; it is refined to the array it copies.
    assert!(!source.contains("-> Int63 throws"), "{source}");
    assert_contains(source, "string::eq(&");
    assert_contains(source, "array::iter(&");
}

#[test]
fn bench_02_merge_sort() {
    compile_benchmark(&format!(
        r"class State {{ values: int[] }}
{PREPARE}
function run(state: State) -> string {{
    let a: int[] = [];
    let b: int[] = [];
    for (let value in state.values) {{ a.push(value); b.push(0); }}
    let width = 1;
    let n = a.length();
    while (width < n) {{
        let lo = 0;
        while (lo < n) {{
            let mid = lo + width;
            if (mid > n) {{ mid = n; }}
            let hi = lo + 2 * width;
            if (hi > n) {{ hi = n; }}
            let i = lo;
            let j = mid;
            let k = lo;
            while (k < hi) {{
                if (i < mid && (j >= hi || a[i] <= a[j])) {{
                    b[k] = a[i]; i += 1;
                }} else {{ b[k] = a[j]; j += 1; }}
                k += 1;
            }}
            lo += 2 * width;
        }}
        let tmp = a; a = b; b = tmp;
        width *= 2;
    }}
    baml.json.to_string(a)
}}"
    ));
}

#[test]
fn bench_04_allocation_retention() {
    let module = compile_benchmark(&format!(
        r"class Cell {{ value: int, payload: int[] }}
class State {{ batches: int, width: int, retain: int }}
{PREPARE}
function run(s: State) -> string {{
    let retained: Cell[][] = [];
    let checksum = 0;
    let batch = 0;
    while (batch < s.batches) {{
        let cells: Cell[] = [];
        let i = 0;
        while (i < s.width) {{
            let cell = Cell {{ value: batch + i, payload: [i, i + 1, i + 2, i + 3] }};
            let alias = cell;
            alias.value += 1;
            alias.payload[0] += 1;
            cells.push(cell);
            i += 1;
        }}
        for (let cell in cells) {{ checksum += cell.value + cell.payload[0]; }}
        if (s.retain > 0) {{
            if (retained.length() < s.retain) {{ retained.push(cells); }}
            else {{ retained[batch % s.retain] = cells; }}
        }}
        batch += 1;
    }}
    let live_sum = 0;
    for (let cells in retained) {{
        for (let cell in cells) {{ live_sum += cell.value + cell.payload[0]; }}
    }}
    baml.json.to_string([checksum, live_sum])
}}"
    ));
    let source = &module.rust_source;
    assert_contains(source, "pub struct user_Cell {");
    assert_contains(source, "Shared<Vec<Shared<Vec<Shared<user_Cell>>>>>");
    assert_contains(source, "shared(user_Cell {");
    assert_contains(
        source,
        ": bex_lang::array::Iter<Shared<Vec<Shared<user_Cell>>>>;",
    );
    assert_eq!(module.classes.len(), 2);
}

#[test]
fn bench_07_function_calls() {
    let module = compile_benchmark(&format!(
        r"class State {{ count: int, seed: int }}
{PREPARE}
function leaf(value: int) -> int {{ value + 1 }}
function run(state: State) -> string {{
    let value = state.seed;
    let i = 0;
    while (i < state.count) {{ value = leaf(value); i += 1; }}
    value.to_string()
}}"
    ));
    assert_contains(&module.rust_source, "= user_leaf(_");
    assert_eq!(module.functions.len(), 3);
}

#[test]
fn bench_09_json_hello() {
    let module = compile_benchmark(&format!(
        r"class State {{ message: string }}
{PREPARE}
function run(state: State) -> string {{ baml.json.to_string(state) }}"
    ));
    assert_contains(&module.rust_source, "= json::to_string(&_1)?;");
}

const QUICK_SORT_RUN: &str = r"
function run(state: State) -> string {
    let a: int[] = [];
    for (let v in state.values) { a.push(v); }
    let lows: int[] = [0]; let highs: int[] = [a.length() - 1]; let top = 1;
    while (top > 0) {
        top -= 1; let lo = lows[top]; let hi = highs[top];
        if (lo < hi) {
            let pivot = a[lo + (hi - lo) / 2];
            let lt = lo; let i = lo; let gt = hi;
            while (i <= gt) {
                if (a[i] < pivot) { let v = a[lt]; a[lt] = a[i]; a[i] = v; lt += 1; i += 1; }
                else if (a[i] > pivot) { let v = a[gt]; a[gt] = a[i]; a[i] = v; gt -= 1; }
                else { i += 1; }
            }
            if (top == lows.length()) { lows.push(lo); highs.push(lt - 1); }
            else { lows[top] = lo; highs[top] = lt - 1; }
            top += 1;
            if (top == lows.length()) { lows.push(gt + 1); highs.push(hi); }
            else { lows[top] = gt + 1; highs[top] = hi; }
            top += 1;
        }
    }
    baml.json.to_string(a)
}";

#[test]
fn bench_10_quick_sort() {
    compile_benchmark(&format!(
        "class State {{ values: int[] }}\n{PREPARE}{QUICK_SORT_RUN}"
    ));
}

#[test]
fn bench_11_generate_sort() {
    let module = compile_benchmark(&format!(
        r"class State {{ count: int seed: int }}
{PREPARE}
function run(s: State) -> string {{
 let a: int[] = []; let value = s.seed;
 for (let i = 0; i < s.count; i += 1) {{ value = (value * 48271) % 1000000007; a.push(value); }}
 a.sort(); baml.json.to_string(a)
}}"
    ));
    assert_contains(&module.rust_source, "array::sort_int(&_2);");
}

const SORT_KERNEL: &str = r"
function sort_kernel(a: int[], lows: int[], highs: int[]) -> int {
    let top = 1;
    lows[0] = 0; highs[0] = a.length() - 1;
    while (top > 0) {
        top -= 1; let lo = lows[top]; let hi = highs[top];
        if (lo < hi) {
            let pivot = a[lo + (hi - lo) / 2];
            let lt = lo; let i = lo; let gt = hi;
            while (i <= gt) {
                if (a[i] < pivot) { let v = a[lt]; a[lt] = a[i]; a[i] = v; lt += 1; i += 1; }
                else if (a[i] > pivot) { let v = a[gt]; a[gt] = a[i]; a[i] = v; gt -= 1; }
                else { i += 1; }
            }
            if (lt - lo > hi - gt) {
                lows[top] = lo; highs[top] = lt - 1; top += 1;
                lows[top] = gt + 1; highs[top] = hi; top += 1;
            } else {
                lows[top] = gt + 1; highs[top] = hi; top += 1;
                lows[top] = lo; highs[top] = lt - 1; top += 1;
            }
        }
    }
    a.length()
}";

#[test]
fn bench_12_quick_sort_native() {
    let module = compile_benchmark(&format!(
        r"class State {{ values: int[] }}
{PREPARE}{SORT_KERNEL}
function run(state: State) -> string {{
    let a: int[] = [];
    for (let v in state.values) {{ a.push(v); }}
    let lows: int[] = []; let highs: int[] = [];
    for (let i = 0; i < 64; i += 1) {{ lows.push(0); highs.push(0); }}
    if (a.length() > 1) {{ sort_kernel(a, lows, highs); }}
    baml.json.to_string(a)
}}"
    ));
    assert_contains(
        &module.rust_source,
        "pub fn user_sort_kernel(\n    mut _1: Shared<Vec<Int63>>,\n    mut _2: Shared<Vec<Int63>>,\n    mut _3: Shared<Vec<Int63>>,\n) -> Result<Int63, Thrown>",
    );
}

#[test]
fn bench_13_sort_kernel_only() {
    let module = compile_benchmark(&format!(
        r"class State {{ values: int[] }}
{PREPARE}{SORT_KERNEL}
function bench(src: int[], a: int[], lows: int[], highs: int[], reps: int) -> int {{
    let check = 0;
    for (let r = 0; r < reps; r += 1) {{
        for (let i = 0; i < src.length(); i += 1) {{ a[i] = src[i]; }}
        sort_kernel(a, lows, highs);
        check += a[0] + a[a.length() - 1];
    }}
    check
}}
function run(state: State) -> string {{
    let src: int[] = []; let a: int[] = [];
    for (let v in state.values) {{ src.push(v); a.push(v); }}
    let lows: int[] = []; let highs: int[] = [];
    for (let i = 0; i < 64; i += 1) {{ lows.push(0); highs.push(0); }}
    let check = 0;
    if (a.length() > 1) {{ check = bench(src, a, lows, highs, 16); }}
    baml.json.to_string(check)
}}"
    ));
    let names: Vec<&str> = module
        .functions
        .iter()
        .map(|f| f.rust_name.as_str())
        .collect();
    assert_eq!(
        names,
        ["user_prepare", "user_sort_kernel", "user_bench", "user_run"]
    );
}

// ── Rejections ──────────────────────────────────────────────────────────────

#[test]
fn rejects_map() {
    let rejection = reject("function f(m: map<string, int>) -> int { m.length() }", "f");
    assert_unsupported(&rejection, "parameter of type `map<string, int>`: map");
}

#[test]
fn rejects_map_local() {
    let rejection = reject(
        r#"function f() -> int { let m: map<string, int> = { "a": 1 }; m.length() }"#,
        "f",
    );
    assert_unsupported(&rejection, "map");
}

#[test]
fn rejects_enum() {
    let rejection = reject(
        "enum Color { Red, Blue }\nfunction f(c: Color) -> bool { c == Color.Red }",
        "f",
    );
    assert_unsupported(&rejection, "parameter of type `Color`: enum");
}

#[test]
fn rejects_generic_class() {
    let rejection = reject(
        "class Box<T> { value: T }\nfunction f(b: Box<int>) -> int { b.value }",
        "f",
    );
    assert_unsupported(&rejection, "generic class");
}

#[test]
fn rejects_class_with_a_map_field() {
    let rejection = reject(
        "class S { m: map<string, int> }\nfunction f(s: S) -> int { 1 }",
        "f",
    );
    assert_unsupported(
        &rejection,
        "class `user.S` field `m` of type `map<string, int>`: map",
    );
}

#[test]
fn rejects_wide_union() {
    let rejection = reject("function f(x: int | string) -> int { 1 }", "f");
    assert_unsupported(&rejection, "union other than `T | null`");
}

#[test]
fn rejects_bigint() {
    let rejection = reject("function f(x: bigint) -> bigint { x }", "f");
    assert_unsupported(&rejection, "bigint");
}

#[test]
fn rejects_unknown_builtin_by_link_name() {
    let rejection = reject("function f(xs: int[]) -> int | null { xs.pop() }", "f");
    assert_unsupported(&rejection, "unsupported builtin `baml.Array.pop`");
}

#[test]
fn rejects_sort_on_a_class_array() {
    let rejection = reject(
        r"
class C { n: int }
implement baml.ops.Compare for C { function cmp(self, other: C) -> baml.ops.Ordering { self.n.cmp(other.n) } }
function f(xs: C[]) -> int { xs.sort(); xs.length() }
",
        "f",
    );
    assert_unsupported(&rejection, "`sort` on a `user.C[]`");
}

#[test]
fn rejects_catch() {
    let rejection = reject(
        r#"
function f(n: int) -> int {
    {
        if (n < 0) { baml.sys.panic("negative"); }
        n
    } catch (error) {
        let error: baml.panics.UserPanic => 0
    }
}
"#,
        "f",
    );
    assert_unsupported(&rejection, "catch");
}

#[test]
fn rejects_defer() {
    let rejection = reject(
        r"
function f(n: int) -> int {
    defer { let x = 1; }
    n
}
",
        "f",
    );
    assert_unsupported(&rejection, "defer");
}

#[test]
fn rejects_spawn() {
    let rejection = reject(
        r"
function work(n: int) -> int { n + 1 }
function f(n: int) -> int {
    let fut = spawn { work(n) };
    await fut
}
",
        "f",
    );
    assert_unsupported(&rejection, "spawn");
}

#[test]
fn rejects_default_parameter() {
    let rejection = reject("function f(n: int = 1) -> int { n }", "f");
    assert_unsupported(&rejection, "default parameter");
}

#[test]
fn rejects_generic_function() {
    let rejection = reject("function f<T>(x: T) -> T { x }", "f");
    assert_unsupported(&rejection, "generic function");
}

#[test]
fn rejects_method() {
    let source = r"
class C { n: int function get(self) -> int { self.n } }
function f(c: C) -> int { c.get() }
";
    let db = setup_test_db(source);
    assert_no_user_diagnostic_errors(&db);
    let loc = function_named(&db, "f");
    let rejection = compile(&db, loc).expect_err("a method call is outside the subset");
    assert_unsupported(&rejection, "method");
}

#[test]
fn rejects_recursion() {
    let db = setup_test_db(
        r"
function even(n: int) -> bool { if (n == 0) { true } else { odd(n - 1) } }
function odd(n: int) -> bool { if (n == 0) { false } else { even(n - 1) } }
",
    );
    assert_no_user_diagnostic_errors(&db);
    let loc = function_named(&db, "even");
    // `admit` passes each function alone; only the call graph sees the cycle.
    admit(&db, loc).expect("each body is in the subset on its own");
    let rejection = compile(&db, loc).unwrap_err();
    assert_unsupported(&rejection, "recursion: user.even -> user.odd -> user.even");
}

#[test]
fn rejects_self_recursion() {
    let db = setup_test_db(
        "function fact(n: int) -> int { if (n <= 1) { 1 } else { n * fact(n - 1) } }",
    );
    assert_no_user_diagnostic_errors(&db);
    let loc = function_named(&db, "fact");
    admit(&db, loc).expect("a self-recursive body is in the subset on its own");
    let rejection = compile(&db, loc).unwrap_err();
    assert_unsupported(&rejection, "recursion: user.fact -> user.fact");
}

#[test]
fn nullable_coercions_wrap_in_some() {
    let module = compile_entry(
        r#"
class Row { name: string | null, count: int | null }
function pick(n: int) -> int | null { if (n > 0) { n } else { null } }
function f(xs: (int | null)[]) -> int | null {
    let row = Row { name: "a", count: 1 };
    row.count = 2;
    let ys: (int | null)[] = [1, null];
    let total: int | null = xs.length() + ys.length();
    let picked: int | null = pick(3);
    picked = pick(4) ;
    if (total == null) { picked } else { total }
}
"#,
        "f",
    );
    let source = &module.rust_source;
    assert_contains(source, "name: Some(string::from_literal(\"a\"))");
    assert_contains(source, "count: Some(");
    assert_contains(source, "= Some(int::add(");
    assert_contains(source, "Vec::from([");
    assert!(source.matches("Some(const {").count() >= 2, "{source}");
    assert_contains(source, "None,");
    assert_contains(source, "user_pick(");
    assert_contains(source, "-> Result<Option<Int63>, Thrown>");
}

#[test]
fn rejects_lambda() {
    let rejection = reject(
        r"
function f(n: int) -> int {
    let add = (x: int) -> int { x + n };
    add(1)
}
",
        "f",
    );
    assert_unsupported(&rejection, "lambda");
}

#[test]
fn rejects_computed_panic_message() {
    let rejection = reject(
        r#"
function f(n: int) -> int {
    let message = "bad";
    if (n < 0) { baml.sys.panic(message); }
    n
}
"#,
        "f",
    );
    assert_unsupported(&rejection, "panic with a computed message");
}

#[test]
fn rejects_callee_outside_the_subset() {
    let source = r"
function inner(m: map<string, int>) -> int { m.length() }
function outer(n: int) -> int { inner({}) + n }
";
    let db = setup_test_db(source);
    assert_no_user_diagnostic_errors(&db);
    let loc = function_named(&db, "outer");
    let rejection = compile(&db, loc).expect_err("callee is rejected");
    assert_unsupported(&rejection, "user.outer");
}
