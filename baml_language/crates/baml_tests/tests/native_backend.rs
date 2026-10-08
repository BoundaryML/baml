//! The native Rust backend (`baml_compiler2_rust`) from BAML source: what
//! compiles, what the output looks like, and what is rejected. The backend's
//! structurizer and must-initialization analyses have unit tests in-crate;
//! these tests go through the compiler on real programs.

use baml_db::{
    ProjectDatabase,
    baml_compiler2_hir::{
        item_data::{file_functions, function_data},
        loc::FunctionLoc,
    },
    baml_compiler2_rust::{
        self as native, NativeModule, Rejection, Scalar, admit, compile, compile_many, rust_name,
        rust_name_for,
    },
    testing::{assert_no_user_diagnostic_errors, setup_test_db},
};

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
    let db = Box::leak(Box::new(setup_test_db(source)));
    assert_no_user_diagnostic_errors(db);
    let loc = function_named(db, entry);
    // The backend parses its own output with `syn` before returning it (a
    // failure is `Rejection::Invalid`), so `Ok` means well-formed Rust.
    let module = compile(db, loc).unwrap_or_else(|rejection| panic!("{rejection}"));
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
    assert!(source.contains("pub fn user_nested(mut _1: Int63) -> Result<Int63, Panic>"));
    assert_eq!(source.matches(": loop {").count(), 2, "{source}");
    assert!(source.contains("continue 'bb"), "{source}");
    assert!(source.contains("break 'bb"), "{source}");
    assert!(source.contains("int::add("), "{source}");
    assert!(source.contains("int::rem("), "{source}");
    assert_eq!(module.functions.len(), 1);
    assert_eq!(
        module.functions[0].params,
        vec![("n".to_string(), Scalar::Int)]
    );
    assert_eq!(module.functions[0].ret, Some(Scalar::Int));
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
    assert!(source.contains("match _1.get() {"), "{source}");
    assert!(source.contains("0i64 =>"), "{source}");
    assert!(source.contains("_ =>"), "{source}");
    assert!(source.contains("int::neg("), "{source}");
    assert!(source.contains("return Ok(_0);"), "{source}");
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
    assert!(
        source.contains("_1 == true") || source.contains("_1 == false"),
        "{source}"
    );
    assert!(source.contains("return Err(Panic::Unreachable)") || !source.contains("Unreachable"));
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
    assert!(source.contains("-> Result<bool, Panic>"), "{source}");
    assert!(source.contains("= false;"), "and short edge: {source}");
    assert!(source.contains("= true;"), "or short edge: {source}");
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
    assert!(source.contains(": loop {"), "{source}");
    assert!(source.contains("return Ok(_0);"), "{source}");
    // The loop's own exit edge assigns `null` to the `int` return place: the
    // checker's proof that the edge is dead, emitted as an unreachable panic.
    assert!(
        source.contains("return Err(Panic::Unreachable);"),
        "{source}"
    );
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
    assert!(source.contains("= user_square(_"), "{source}");
    assert!(source.contains(")?;"), "{source}");
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
    assert!(
        source.contains(
            "return Err(Panic::UserPanic {\n            message: String::from(\"negative input\"),\n        });"
        ) || source.contains("String::from(\"negative input\")"),
        "{source}"
    );
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
    assert!(
        source.contains("pub fn user_noop(mut _1: Int63) -> Result<(), Panic>"),
        "{source}"
    );
    assert!(source.contains("= user_noop(_1)?;"), "{source}");
    assert_eq!(module.functions[0].ret, None);
}

#[test]
fn int_literals_fold_at_compile_time() {
    let module = compile_entry("function k() -> int { 42 }", "k");
    assert!(
        module
            .rust_source
            .contains("const { Int63::new(42i64).expect(\"int literal fits int\") }"),
        "{}",
        module.rust_source
    );
}

#[test]
fn rejects_string_parameter() {
    let rejection = reject("function f(s: string) -> int { 1 }", "f");
    assert_unsupported(&rejection, "parameter of type `string`");
}

#[test]
fn rejects_float() {
    let rejection = reject("function f(x: float) -> float { x }", "f");
    assert_unsupported(&rejection, "float");
}

#[test]
fn rejects_array() {
    let rejection = reject("function f(xs: int[]) -> int { xs.length() }", "f");
    assert_unsupported(&rejection, "int[]");
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
    let total = n;
    {
        defer { total = total + 1; }
        total = total * 2;
    }
    total
}
",
        "f",
    );
    assert_unsupported(&rejection, "defer");
}

#[test]
fn rejects_default_parameter() {
    let rejection = reject("function f(x: int = 3) -> int { x }", "f");
    assert_unsupported(&rejection, "default parameter");
}

#[test]
fn rejects_generic_function() {
    let rejection = reject("function f<T>(x: int) -> int { x }", "f");
    assert_unsupported(&rejection, "generic");
}

#[test]
fn rejects_method() {
    let db = setup_test_db(
        r"
class Counter {
    value: int
    function get(self) -> int { self.value }
}
",
    );
    assert_no_user_diagnostic_errors(&db);
    let loc = function_named(&db, "get");
    assert_unsupported(&admit(&db, loc).unwrap_err(), "method");
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
    let Rejection::Unsupported(reason) = &rejection else {
        panic!("{rejection}");
    };
    assert!(reason.contains("recursion"), "{reason}");
    assert!(
        reason.contains("user.even -> user.odd -> user.even"),
        "{reason}"
    );
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
fn rejects_lambda() {
    let rejection = reject(
        r"
function f(n: int) -> int {
    let add = (a: int) => { a + n };
    add(1)
}
",
        "f",
    );
    assert!(
        matches!(rejection, Rejection::Unsupported(_)),
        "{rejection}"
    );
}

#[test]
fn rejects_computed_panic_message() {
    let rejection = reject(
        r#"
function f(n: int) -> int {
    let message = "n=" + "x";
    baml.sys.panic(message)
}
"#,
        "f",
    );
    assert!(
        matches!(rejection, Rejection::Unsupported(_)),
        "{rejection}"
    );
}

#[test]
fn rejects_callee_outside_the_subset() {
    let rejection = reject(
        r#"
function helper(s: string) -> int { s.length() }
function f(n: int) -> int { helper("x") + n }
"#,
        "f",
    );
    assert_unsupported(&rejection, "user.f:");
}

#[test]
fn write_project_lays_out_a_crate() {
    let module = compile_entry(
        r"
function main(n: int, verbose: bool) -> int { if (verbose) { n * 2 } else { n } }
",
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
    assert!(manifest.contains("name = \"native-main\""), "{manifest}");
    assert!(manifest.contains("[workspace]"), "{manifest}");
    assert!(
        manifest.contains("bex_lang = { path = \"/tmp/runtime/bex_lang\" }"),
        "{manifest}"
    );
    assert!(manifest.contains("lto = \"fat\""), "{manifest}");
    assert_eq!(
        std::fs::read_to_string(dir.join("src/lib.rs")).unwrap(),
        module.rust_source
    );
    let shim = std::fs::read_to_string(dir.join("src/main.rs")).unwrap();
    assert!(
        shim.contains("\"--n\" => arg_0 = Some(parse_int(\"n\", value)),"),
        "{shim}"
    );
    assert!(
        shim.contains("\"--verbose\" => arg_1 = Some(parse_bool(\"verbose\", value)),"),
        "{shim}"
    );
    assert!(
        shim.contains("native_main::user_main(value_0, value_1)"),
        "{shim}"
    );
    assert!(shim.contains("uncaught throw"), "{shim}");
    assert!(
        shim.contains("usage: native-main --n <int> --verbose <bool>"),
        "{shim}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("mir.txt")).unwrap(),
        module.mir_dump
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
    assert_eq!(rust_name("9lives"), "_9lives");
    assert_eq!(rust_name(""), "_");
}
