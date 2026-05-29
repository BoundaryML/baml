//! Tests for the `interface` declaration and `implements I { ... }` blocks
//! introduced by BEP-044.
//!
//! These tests exercise the parser + AST lowering + per-file interface
//! validation. They do **not** exercise method dispatch at runtime — that is
//! tracked separately as TIR/MIR work (see BEP-044 §"Method Disambiguation").
//!
//! The shape of each test:
//!   1. Compile a self-contained BAML snippet through the project pipeline.
//!   2. Collect compile errors that originate in the user file.
//!   3. Assert presence (or absence) of specific diagnostic codes / messages.
//!
//! Coverage groups roughly track BEP-044's "Corner Cases" matrix:
//!   • happy-path parsing of `interface`, `requires`, `implements`
//!   • missing required methods
//!   • unknown interface in `implements`
//!   • duplicate `implements` blocks on the same class
//!   • unknown member declared inside `implements`
//!   • interface field namespace/construction rules
//!   • same-name fields from unrelated interfaces remain separate
//!   • `requires` cycles
//!   • interface requirements (parent contracts available through `requires`)
//!   • generic interfaces parse and resolve
//!   • field-only interfaces and empty bodies

use std::collections::HashSet;

use baml_compiler_diagnostics::Severity;
use baml_project::{ProjectDatabase, collect_diagnostics, testing::setup_test_db};
use baml_tests::baml_test;
use baml_type::Ty;
use bex_engine::BexExternalValue;

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Collect compile errors raised in user files. Returns `[E0123] message`
/// strings so tests can assert against the public diagnostic code without
/// being tied to exact wording.
fn collect_compile_errors(source: &str) -> Vec<String> {
    let db = setup_test_db(source);
    collect_compile_errors_from_db(&db)
}

fn collect_compile_errors_multi(files: &[(&str, &str)]) -> Vec<String> {
    let mut db = ProjectDatabase::new();
    db.set_project_root(std::path::Path::new("."));
    for (path, source) in files {
        db.add_file(*path, source);
    }
    collect_compile_errors_from_db(&db)
}

fn collect_compile_errors_from_db(db: &ProjectDatabase) -> Vec<String> {
    let project = db.get_project().expect("project must be set");
    let all_files = db.get_source_files();
    let user_file_ids: HashSet<_> = all_files.iter().map(|f| f.file_id(db)).collect();

    collect_diagnostics(db, project, &all_files)
        .into_iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .filter(|d| {
            d.primary_span()
                .map(|span| user_file_ids.contains(&span.file_id))
                .unwrap_or(false)
        })
        .map(|d| format!("[{}] {}", d.code(), d.message))
        .collect()
}

#[track_caller]
fn assert_compile_error_contains_multi(files: &[(&str, &str)], needle: &str) {
    let errors = collect_compile_errors_multi(files);
    assert!(
        errors.iter().any(|e| e.contains(needle)),
        "expected a compile error containing {needle:?}; got:\n  {}",
        errors.join("\n  ")
    );
}

#[track_caller]
fn assert_no_compile_errors_multi(files: &[(&str, &str)]) {
    let errors = collect_compile_errors_multi(files);
    assert!(
        errors.is_empty(),
        "expected no compile errors; got:\n  {}",
        errors.join("\n  ")
    );
}

#[track_caller]
fn assert_compile_error_code(source: &str, code: &str) {
    let errors = collect_compile_errors(source);
    assert!(
        errors.iter().any(|e| e.starts_with(&format!("[{code}]"))),
        "expected a compile error with code {code}; got:\n  {}",
        errors.join("\n  ")
    );
}

#[track_caller]
fn assert_compile_error_contains(source: &str, needle: &str) {
    let errors = collect_compile_errors(source);
    assert!(
        errors.iter().any(|e| e.contains(needle)),
        "expected a compile error containing {needle:?}; got:\n  {}",
        errors.join("\n  ")
    );
}

#[track_caller]
fn assert_no_compile_errors(source: &str) {
    let errors = collect_compile_errors(source);
    assert!(
        errors.is_empty(),
        "expected no compile errors, got:\n  {}",
        errors.join("\n  ")
    );
}

#[track_caller]
fn assert_no_interface_errors(source: &str) {
    let errors = collect_compile_errors(source);
    // Interface errors all live in the E0112-E0132 range.
    let interface_errors: Vec<_> = errors
        .iter()
        .filter(|e| {
            e.starts_with("[E0112]")
                || e.starts_with("[E0113]")
                || e.starts_with("[E0114]")
                || e.starts_with("[E0115]")
                || e.starts_with("[E0116]")
                || e.starts_with("[E0117]")
                || e.starts_with("[E0118]")
                || e.starts_with("[E0119]")
                || e.starts_with("[E0120]")
                || e.starts_with("[E0121]")
                || e.starts_with("[E0122]")
                || e.starts_with("[E0123]")
                || e.starts_with("[E0124]")
                || e.starts_with("[E0125]")
                || e.starts_with("[E0126]")
                || e.starts_with("[E0127]")
                || e.starts_with("[E0128]")
                || e.starts_with("[E0129]")
                || e.starts_with("[E0130]")
                || e.starts_with("[E0131]")
                || e.starts_with("[E0132]")
        })
        .collect();
    assert!(
        interface_errors.is_empty(),
        "expected no interface-related errors, got:\n  {}",
        interface_errors
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

// ── Group A: happy-path parsing ─────────────────────────────────────────────

#[test]
fn basic_interface_parses() {
    assert_no_interface_errors(
        r#"
        interface Animal {
            name: string
            age: int
            function speak(self) -> string
        }

        class Dog {
            name: string
            age: int
            breed: string
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        "#,
    );
}

#[test]
fn interface_with_only_fields_parses() {
    assert_no_interface_errors(
        r#"
        interface Config {
            host: string
            port: int
        }

        class Server {
            host: string
            port: int
            implements Config {}
        }
        "#,
    );
}

#[test]
fn interface_default_method_inherited() {
    // `display` has a default body; the empty `implements` block inherits it.
    assert_no_interface_errors(
        r#"
        interface Printable {
            function display(self) -> string {
                return "<printable>"
            }
        }

        class Item {
            implements Printable {}
        }
        "#,
    );
}

#[test]
fn interface_requires_aggregates_contracts() {
    assert_no_interface_errors(
        r#"
        interface Named { name: string }
        interface Aged { age: int }
        interface Person requires Named, Aged {
            occupation: string
            function introduce(self) -> string
        }

        class Employee {
            name: string
            age: int
            occupation: string
            salary: float
            implements Named {}
            implements Aged {}
            implements Person {
                function introduce(self) -> string { return "hi" }
            }
        }
        "#,
    );
}

#[test]
fn generic_interface_parses() {
    // A generic interface with a default method and a class that implements
    // a concrete instantiation should parse without interface errors.
    assert_no_interface_errors(
        r#"
        interface Container<T> {
            function size(self) -> int {
                return 0
            }
        }

        class IntBag {
            items: int[]
            implements Container<int> {}
        }
        "#,
    );
}

#[test]
fn class_can_implement_multiple_interfaces() {
    assert_no_interface_errors(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        interface Swimmer {
            function swim(self) -> string
        }

        class Duck {
            color: string
            implements Animal {
                function speak(self) -> string { return "Quack!" }
            }
            implements Swimmer {
                function swim(self) -> string { return "splash" }
            }
        }
        "#,
    );
}

// ── Group B: missing / wrong methods ────────────────────────────────────────

#[test]
fn missing_required_method_is_compile_error() {
    assert_compile_error_code(
        r#"
        interface Animal {
            function speak(self) -> string
        }

        class Incomplete {
            implements Animal {}
        }
        "#,
        "E0113",
    );
}

#[test]
fn missing_required_method_message_names_method_and_interface() {
    assert_compile_error_contains(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Mute {
            implements Animal {}
        }
        "#,
        "required method `speak` of interface `user.Animal`",
    );
}

#[test]
fn implements_unknown_interface_is_compile_error() {
    assert_compile_error_code(
        r#"
        class Ghost {
            implements DoesNotExist {}
        }
        "#,
        "E0112",
    );
}

#[test]
fn implementing_a_class_is_not_allowed() {
    assert_compile_error_code(
        r#"
        class Foo { x: int }
        class Bar {
            implements Foo {}
        }
        "#,
        "E0119",
    );
}

#[test]
fn implementing_an_enum_is_not_allowed() {
    assert_compile_error_code(
        r#"
        enum Color { Red, Green, Blue }
        class Bar {
            implements Color {}
        }
        "#,
        "E0119",
    );
}

#[test]
fn unknown_method_in_implements_block_is_compile_error() {
    assert_compile_error_code(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "woof" }
                function bark(self) -> string { return "bark" }
            }
        }
        "#,
        "E0115",
    );
}

// ── Group C: duplicate / cycle ──────────────────────────────────────────────

#[test]
fn duplicate_implements_block_is_compile_error() {
    assert_compile_error_code(
        r#"
        interface Animal {
            function speak(self) -> string
        }

        class Dog {
            implements Animal {
                function speak(self) -> string { return "a" }
            }
            implements Animal {
                function speak(self) -> string { return "b" }
            }
        }
        "#,
        "E0114",
    );
}

#[test]
fn duplicate_same_generic_interface_instantiation_is_compile_error() {
    assert_compile_error_code(
        r#"
        interface Converter<T> {
            function convert(self) -> T
        }

        class MultiFormat {
            implements Converter<int> {
                function convert(self) -> int { return 1 }
            }
            implements Converter<int> {
                function convert(self) -> int { return 2 }
            }
        }
        "#,
        "E0114",
    );
}

#[test]
fn requires_cycle_is_compile_error() {
    assert_compile_error_code(
        r#"
        interface A requires B {}
        interface B requires A {}
        "#,
        "E0118",
    );
}

#[test]
fn three_way_requires_cycle_is_compile_error() {
    assert_compile_error_code(
        r#"
        interface A requires B {}
        interface B requires C {}
        interface C requires A {}
        "#,
        "E0118",
    );
}

// ── Group D: field namespace rules ───────────────────────────────────────────

#[test]
fn class_own_field_can_shadow_interface_field_with_different_type() {
    assert_zero_compile_errors(
        r#"
        interface Config {
            host: string
        }

        class Server {
            host: int
            config_host: string
            implements Config {
                host as config_host
            }
        }
        "#,
    );
}

#[test]
fn class_field_auto_satisfies_interface_field_with_empty_implements_block() {
    assert_zero_compile_errors(
        r#"
        interface Config {
            host: string
            port: int
        }

        class Server {
            host: string
            port: int
            implements Config {}
        }
        "#,
    );
}

#[test]
fn missing_class_field_for_interface_field_is_error() {
    assert_compile_error_code(
        r#"
        interface Config {
            host: string
            port: int
        }

        class Server {
            host: string
            implements Config {}
        }
        "#,
        "E0124",
    );
}

#[test]
fn two_interfaces_same_field_same_type_are_separate_namespaces() {
    assert_zero_compile_errors(
        r#"
        interface Named { name: string }
        interface Labeled { name: string }

        class Item {
            name: string
            implements Named {}
            implements Labeled {}
        }
        "#,
    );
}

#[test]
fn unrelated_interfaces_same_field_different_types_are_separate_namespaces() {
    assert_zero_compile_errors(
        r#"
        interface HasId { id: string }
        interface HasNumId { id: int }

        class Thing {
            text_id: string
            numeric_id: int
            implements HasId { id as text_id }
            implements HasNumId { id as numeric_id }
        }
        "#,
    );
}

// ── Group E: inheritance / requires ─────────────────────────────────────────

#[test]
fn interface_extends_clause_is_rejected() {
    assert_compile_error_contains(
        r#"
        interface Named {
            name: string
        }
        interface Person extends Named {
            age: int
        }
        "#,
        "extends",
    );
}

#[test]
fn implementing_requires_chain_satisfies_parent_required_methods() {
    // `Person` requires `Named, Aged` and adds `introduce()`. Implementing
    // `Person` must satisfy `introduce` — but not duplicate name/age methods.
    assert_compile_error_contains(
        r#"
        interface Named { name: string }
        interface Aged { age: int }
        interface Person requires Named, Aged {
            occupation: string
            function introduce(self) -> string
        }

        class Employee {
            salary: float
            implements Person {}
        }
        "#,
        "required method `introduce` of interface `user.Person`",
    );
}

#[test]
fn requires_chain_required_method_must_be_provided() {
    // `Person` requires `Greeter`, which has a required method `greet`.
    // Implementing `Person` should require `greet`.
    assert_compile_error_contains(
        r#"
        interface Greeter {
            function greet(self) -> string
        }
        interface Person requires Greeter {
            name: string
        }

        class Bob {
            implements Person {}
        }
        "#,
        "required method `greet`",
    );
}

// ── Group F: misc / regression ──────────────────────────────────────────────

#[test]
fn empty_implements_block_with_all_defaults_is_ok() {
    assert_no_interface_errors(
        r#"
        interface Printable {
            function display(self) -> string { return "x" }
            function verbose(self) -> string { return "y" }
        }

        class Item {
            implements Printable {}
        }
        "#,
    );
}

#[test]
fn interface_with_only_required_methods_parses() {
    assert_no_interface_errors(
        r#"
        interface Closeable {
            function close(self) -> null
        }

        class File {
            handle: int
            implements Closeable {
                function close(self) -> null { return null }
            }
        }
        "#,
    );
}

#[test]
fn class_can_have_methods_outside_of_implements() {
    // Class methods that are not part of any interface are still allowed.
    assert_no_interface_errors(
        r#"
        interface Speaker {
            function speak(self) -> string
        }

        class Dog {
            breed: string

            function helper(self) -> string { return "h" }

            implements Speaker {
                function speak(self) -> string { return "Woof" }
            }
        }
        "#,
    );
}

#[test]
fn diagnostic_codes_in_expected_range() {
    // Regression: every interface diagnostic code we emit must be in the
    // E0112+ range we reserved in `baml_compiler_diagnostics`.
    let bad_cases: &[(&str, &str)] = &[
        // (snippet, expected code)
        (
            "interface I { function f(self) -> string } class C { implements I {} }",
            "E0113",
        ),
        ("class C { implements Missing {} }", "E0112"),
        ("class X { x: int } class C { implements X {} }", "E0119"),
    ];
    for (source, code) in bad_cases {
        assert_compile_error_code(source, code);
    }
}

// ── Group H: nominal subtyping (TIR) ────────────────────────────────────────
//
// `Class C` is a subtype of `Interface I` iff `C implements I` (transitively
// through `requires`). These tests pass the compiler a function parameter
// typed as an interface and check that a class instance is accepted only
// when the class declares `implements`.

#[test]
fn class_can_be_passed_to_interface_param_when_implements() {
    assert_no_interface_errors(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }

        function describe(a: Animal) -> string {
            return "an animal"
        }

        function main() -> string {
            let d = Dog {}
            return describe(d)
        }
        "#,
    );
}

#[test]
fn class_without_implements_cannot_satisfy_interface_param() {
    // Nominal: a class with the right shape but no `implements` block is
    // not a subtype of the interface.
    assert_compile_error_contains(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        // Note: no `implements Animal`.
        class Robot {
            function speak(self) -> string { return "beep" }
        }

        function describe(a: Animal) -> string {
            return "an animal"
        }

        function main() -> string {
            let r = Robot {}
            return describe(r)
        }
        "#,
        "Robot",
    );
}

#[test]
fn method_signature_mismatch_return_type() {
    assert_compile_error_code(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Robot {
            implements Animal {
                function speak(self) -> int { return 42 }
            }
        }
        "#,
        "E0120",
    );
}

#[test]
fn method_signature_mismatch_param_type() {
    assert_compile_error_code(
        r#"
        interface Adder {
            function add(self, a: int, b: int) -> int
        }
        class BadAdder {
            implements Adder {
                function add(self, a: string, b: string) -> int { return 0 }
            }
        }
        "#,
        "E0120",
    );
}

#[test]
fn method_signature_mismatch_missing_throws_annotation() {
    assert_compile_error_code(
        r#"
        class IoError {
            message: string
        }

        interface Fallible {
            function run(self) -> string throws IoError
        }

        class Worker {
            implements Fallible {
                function run(self) -> string {
                    return "ok"
                }
            }
        }
        "#,
        "E0120",
    );
}

#[test]
fn method_signature_match_is_ok() {
    assert_no_interface_errors(
        r#"
        interface Adder {
            function add(self, a: int, b: int) -> int
        }
        class GoodAdder {
            implements Adder {
                function add(self, a: int, b: int) -> int { return a + b }
            }
        }
        "#,
    );
}

#[test]
fn method_signature_match_preserves_throws_annotation() {
    assert_no_interface_errors(
        r#"
        class IoError {
            message: string
        }

        interface Fallible {
            function run(self) -> string throws IoError
        }

        class Worker {
            implements Fallible {
                function run(self) -> string throws IoError {
                    return "ok"
                }
            }
        }
        "#,
    );
}

#[test]
fn interface_default_method_body_gets_exhaustiveness_checking() {
    let errors = collect_compile_errors(
        r#"
        interface Labels {
            function label(self, value: bool) -> string {
                return match (value) {
                    true => "yes"
                }
            }
        }

        class Thing {
            implements Labels {}
        }
        "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("E0062") || e.to_lowercase().contains("non-exhaustive")),
        "expected a NonExhaustiveMatch error, got:\n  {}",
        errors.join("\n  ")
    );
}

#[test]
fn calling_method_through_interface_typed_param() {
    // A function parameter typed as the interface should expose the
    // interface's methods on the value, even though the value is a concrete
    // class. Currently this only checks the interface's `required_methods`
    // can be referenced; method-call resolution falls through to the
    // class's flattened methods list when the call is unqualified.
    //
    // This test pins down that interface-typed parameters accept concrete
    // instances and that fields declared on the interface are visible
    // through the interface-typed variable.
    assert_no_interface_errors(
        r#"
        interface Animal {
            name: string
            function speak(self) -> string
        }

        class Dog {
            name: string
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }

        function name_of(a: Animal) -> string {
            return a.name
        }

        function main() -> string {
            let d = Dog { name: "Rex" }
            return name_of(d)
        }
        "#,
    );
}

// ── Group K: method ambiguity (BEP-044 §Method Disambiguation) ─────────────

#[test]
fn class_with_same_named_methods_from_two_interfaces_compiles() {
    // BEP-044 §"Method Disambiguation": declaring `encode` in two
    // `implements` blocks is fine — the class compiles. Ambiguity is
    // only surfaced at the call site if an unqualified call has no way
    // to disambiguate.
    assert_no_interface_errors(
        r#"
        interface Serializer {
            function encode(self) -> string
        }
        interface BinarySerializer {
            function encode(self) -> string
        }

        class Hybrid {
            implements Serializer {
                function encode(self) -> string { return "json" }
            }
            implements BinarySerializer {
                function encode(self) -> string { return "bin" }
            }
        }
        "#,
    );
}

#[test]
fn unqualified_call_on_ambiguous_class_errors() {
    // Calling `h.encode()` unqualified must error because the call site
    // has no information to pick between Serializer's and BinarySerializer's
    // method. The diagnostic should list every contributing interface.
    let errors = collect_compile_errors(
        r#"
        interface Serializer {
            function encode(self) -> string
        }
        interface BinarySerializer {
            function encode(self) -> string
        }
        class Hybrid {
            implements Serializer {
                function encode(self) -> string { return "json" }
            }
            implements BinarySerializer {
                function encode(self) -> string { return "bin" }
            }
        }
        function main() -> string {
            let h = Hybrid {}
            return h.encode()
        }
        "#,
    );
    assert!(
        errors.iter().any(|e| e.starts_with("[E0121]")
            && e.contains("`Serializer`")
            && e.contains("`BinarySerializer`")),
        "expected an E0121 diagnostic at the call site naming both \
         interfaces, got:\n  {}",
        errors.join("\n  ")
    );
}

#[test]
fn three_way_unqualified_call_lists_every_source() {
    // Three-or-more-way clash: the call-site diagnostic must list every
    // contributing interface, not just two.
    let errors = collect_compile_errors(
        r#"
        interface A {
            function id(self) -> string
        }
        interface B {
            function id(self) -> string
        }
        interface C {
            function id(self) -> string
        }

        class Tri {
            implements A {
                function id(self) -> string { return "a" }
            }
            implements B {
                function id(self) -> string { return "b" }
            }
            implements C {
                function id(self) -> string { return "c" }
            }
        }
        function main() -> string {
            let t = Tri {}
            return t.id()
        }
        "#,
    );
    assert!(
        errors.iter().any(|e| e.starts_with("[E0121]")
            && e.contains("`A`")
            && e.contains("`B`")
            && e.contains("`C`")),
        "expected an E0121 diagnostic naming all three interfaces, got:\n  {}",
        errors.join("\n  ")
    );
}

#[test]
fn class_own_method_does_not_resolve_ambiguous_interface_method_call() {
    assert_compile_error_code(
        r#"
        interface Serializer {
            function encode(self) -> string
        }
        interface BinarySerializer {
            function encode(self) -> string
        }
        class Hybrid {
            function encode(self) -> string { return "class" }
            implements Serializer {
                function encode(self) -> string { return "json" }
            }
            implements BinarySerializer {
                function encode(self) -> string { return "bin" }
            }
        }
        function main() -> string {
            let h = Hybrid {}
            return h.encode()
        }
        "#,
        "E0121",
    );
}

#[test]
fn distinct_method_names_across_interfaces_is_not_ambiguous() {
    assert_no_interface_errors(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        interface Swimmer {
            function swim(self) -> string
        }

        class Duck {
            implements Animal {
                function speak(self) -> string { return "Quack!" }
            }
            implements Swimmer {
                function swim(self) -> string { return "splash" }
            }
        }
        "#,
    );
}

// ── Group J: match narrowing on interface types ─────────────────────────────

#[test]
fn match_with_catchall_on_interface_is_exhaustive() {
    // Interfaces are open; a `_` arm is required. With one present the
    // match must type-check cleanly.
    assert_no_interface_errors(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }

        function describe(a: Animal) -> string {
            return match (a) {
                let d: Dog => "dog"
                _ => "other"
            }
        }
        "#,
    );
}

#[test]
fn match_without_catchall_on_interface_is_compile_error() {
    // Without `_`, exhaustiveness should fail.
    let errors = collect_compile_errors(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        class Cat {
            implements Animal {
                function speak(self) -> string { return "Meow." }
            }
        }

        function describe(a: Animal) -> string {
            return match (a) {
                let d: Dog => "dog"
                let c: Cat => "cat"
            }
        }
        "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("E0062") || e.to_lowercase().contains("non-exhaustive")),
        "expected a NonExhaustiveMatch error, got:\n  {}",
        errors.join("\n  ")
    );
}

#[test]
fn match_narrows_interface_to_concrete_class() {
    // Inside `let d: Dog`, the binding `d` is typed as `Dog`, letting
    // class-specific fields like `breed` be accessed. Uses `assert_zero_compile_errors`
    // (not the interface-range-only helper) so a parse error in the match arm
    // can't slip through — see finding #30.
    assert_zero_compile_errors(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            breed: string
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }

        function describe(a: Animal) -> string {
            return match (a) {
                let d: Dog => d.breed
                _ => "other"
            }
        }
        "#,
    );
}

// ── Group I: reflection (BEP-044 §Runtime Reflection) ──────────────────────

#[test]
fn reflect_class_implements_interface() {
    // `Dog.implements(Animal)` is true; `Robot.implements(Animal)` is false.
    assert_no_interface_errors(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }

        function main() -> bool {
            let dog_t = reflect.type_of<Dog>()
            let animal_t = reflect.type_of<Animal>()
            return dog_t.implements(animal_t)
        }
        "#,
    );
}

#[test]
fn reflect_implemented_by_is_reverse() {
    assert_no_interface_errors(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }

        function main() -> bool {
            let dog_t = reflect.type_of<Dog>()
            let animal_t = reflect.type_of<Animal>()
            return animal_t.implemented_by(dog_t)
        }
        "#,
    );
}

#[test]
fn reflect_implementors_returns_type_array() {
    // `Animal.implementors()` returns `type[]`. We just check the program
    // compiles + the types line up; the runtime test below verifies semantics.
    assert_no_interface_errors(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        class Cat {
            implements Animal {
                function speak(self) -> string { return "Meow." }
            }
        }

        function main() -> int {
            let animal_t = reflect.type_of<Animal>()
            return animal_t.implementors().length()
        }
        "#,
    );
}

#[test]
fn calling_implements_block_method_works() {
    // The class methods array is flattened to include `implements` block
    // methods. Calling them via `obj.method()` should resolve through the
    // normal class-method-lookup path without any extra plumbing.
    assert_no_interface_errors(
        r#"
        interface Animal {
            function speak(self) -> string
        }

        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }

        function main() -> string {
            let d = Dog {}
            return d.speak()
        }
        "#,
    );
}

#[test]
fn nominal_subtype_via_requires_chain() {
    // `Person requires Named`, so the implementor must explicitly implement
    // both interfaces. A `Person` value can then be used where `Named` is
    // expected through the requires-chain subtype relation.
    assert_no_interface_errors(
        r#"
        interface Named { name: string }
        interface Person requires Named {
            function introduce(self) -> string
        }

        class Employee {
            name: string
            implements Named {}
            implements Person {
                function introduce(self) -> string { return "hello" }
            }
        }

        function get_name(n: Named) -> string {
            return "x"
        }

        function main() -> string {
            let e = Employee { name: "Alice" }
            return get_name(e)
        }
        "#,
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Runtime tests (BEP-044). Each test below executes a small program through
// the BAML VM and pins the returned value. These cover the behavioural
// guarantees that compile-time checks above cannot — dispatch, default body
// resolution, `.as<I>` projections, match narrowing, generic monomorphisation,
// and reflection results.
// ─────────────────────────────────────────────────────────────────────────────

// ── Group L: interface field namespaces ──────────────────────────────────────

#[tokio::test]
async fn interface_fields_constructed_with_class_keys() {
    let output = baml_test!(
        r#"
        interface Config {
            host: string
            port: int
        }
        class Server {
            host: string
            port: int
            max_connections: int
            implements Config {}
        }
        function main() -> bool {
            let s = Server { host: "localhost", port: 8080, max_connections: 50 }
            let c: Config = s
            return c.host == "localhost"
                && c.port == 8080
                && s.host == "localhost"
                && s.max_connections == 50
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

#[test]
fn qualified_interface_field_construction_is_compile_error() {
    assert_compile_error_contains(
        r#"
        interface Config {
            host: string
        }
        class Server {
            host: string
            implements Config {}
        }
        function main() -> string {
            let s = Server { Config.host: "localhost" }
            return s.host
        }
        "#,
        "Config.host",
    );
}

#[test]
fn simple_interface_field_view_construction_is_compile_error() {
    assert_compile_error_contains(
        r#"
        interface Named {
            name: string
        }
        class Person {
            title: string
            implements Named {
                name as title
            }
        }
        function main() -> string {
            let p = Person { name: "Ada" }
            return p.title
        }
        "#,
        "use class field `title`",
    );
}

#[tokio::test]
async fn aliased_interface_fields_do_not_create_concrete_runtime_slots() {
    let output = baml_test!(
        r#"
        interface Named {
            name: string
        }
        class Person {
            title: string
            implements Named {
                name as title
            }
        }
        function main() -> Person {
            return Person { title: "Ada" }
        }
        "#
    );

    let Ok(BexExternalValue::Instance { class_name, fields }) = output.result else {
        panic!("expected instance, got: {:?}", output.result);
    };
    assert_eq!(class_name, "user.Person");
    assert_eq!(
        fields.get("title"),
        Some(&BexExternalValue::String("Ada".into()))
    );
    assert!(
        fields.get("name").is_none(),
        "interface field `name` must remain a view, not a concrete runtime slot"
    );
}

#[tokio::test]
async fn interface_return_uses_concrete_implementor_field_shape() {
    let output = baml_test!(
        r#"
        interface Named {
            name: string
        }
        class Person {
            title: string
            implements Named {
                name as title
            }
        }
        function main() -> Named {
            return Person { title: "Ada" }
        }
        "#
    );

    let Ok(BexExternalValue::Instance { class_name, fields }) = output.result else {
        panic!("expected instance, got: {:?}", output.result);
    };
    assert_eq!(class_name, "user.Person");
    assert_eq!(
        fields.get("title"),
        Some(&BexExternalValue::String("Ada".into()))
    );
    assert!(
        fields.get("name").is_none(),
        "interface return should serialize the concrete implementor shape"
    );
}

#[test]
fn interface_field_type_must_match_invariantly() {
    assert_compile_error_code(
        r#"
        interface Measured {
            value: int | string
        }
        class Count {
            value: int
            implements Measured {}
        }
        "#,
        "E0116",
    );
}

#[test]
fn interface_field_union_order_is_exactly_equivalent() {
    assert_zero_compile_errors(
        r#"
        interface Measured {
            value: int | string
        }
        class Reading {
            value: string | int
            implements Measured {}
        }
        "#,
    );
}

#[test]
fn generic_interface_field_union_order_is_exactly_equivalent() {
    assert_zero_compile_errors(
        r#"
        interface Measured<T, E> {
            value: T | E
        }
        class Reading<T, E> {
            value: E | T
            implements Measured<T, E> {}
        }
        "#,
    );
}

#[tokio::test]
async fn class_own_field_shadowing_interface_field_is_separate_at_runtime() {
    let output = baml_test!(
        r#"
        interface Config {
            host: string
        }
        class Server {
            host: int
            config_host: string
            implements Config {
                host as config_host
            }
        }
        function main() -> bool {
            let s = Server { host: 0, config_host: "localhost" }
            let c: Config = s
            return s.host == 0 && c.host == "localhost"
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

#[tokio::test]
async fn two_interfaces_same_field_not_merged_runtime() {
    let output = baml_test!(
        r#"
        interface Named { name: string }
        interface Labeled { name: string }
        class Item {
            named_name: string
            labeled_name: string
            implements Named { name as named_name }
            implements Labeled { name as labeled_name }
        }
        function main() -> bool {
            let i = Item { named_name: "widget", labeled_name: "WIDGET-001" }
            let n: Named = i
            let l: Labeled = i
            return n.name == "widget" && l.name == "WIDGET-001"
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

#[test]
fn unqualified_same_name_interface_field_access_is_ambiguous() {
    assert_compile_error_contains(
        r#"
        interface Named { name: string }
        interface Labeled { name: string }
        class Item {
            named_name: string
            labeled_name: string
            implements Named { name as named_name }
            implements Labeled { name as labeled_name }
        }
        function main() -> string {
            let i = Item { named_name: "widget", labeled_name: "WIDGET-001" }
            return i.name
        }
        "#,
        "as<Named>",
    );
}

#[tokio::test]
async fn same_field_name_different_interface_types_not_conflicting_runtime() {
    let output = baml_test!(
        r#"
        interface HasId { id: string }
        interface HasNumId { id: int }
        class Thing {
            text_id: string
            numeric_id: int
            implements HasId { id as text_id }
            implements HasNumId { id as numeric_id }
        }
        function main() -> bool {
            let t = Thing { text_id: "abc", numeric_id: 42 }
            let text: HasId = t
            let numeric: HasNumId = t
            return text.id == "abc" && numeric.id == 42
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

#[tokio::test]
async fn same_generic_interface_field_links_select_matching_type_args_runtime() {
    let output = baml_test!(
        r#"
        interface Slot<T> {
            value: T
        }
        class Pair {
            int_value: int
            string_value: string
            implements Slot<int> {
                value as int_value
            }
            implements Slot<string> {
                value as string_value
            }
        }
        function main() -> bool {
            let p = Pair { int_value: 7, string_value: "seven" }
            let i: Slot<int> = p
            let s: Slot<string> = p
            return i.value == 7 && s.value == "seven"
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

#[tokio::test]
async fn generic_interface_field_links_preserve_swapped_type_var_identity_runtime() {
    let output = baml_test!(
        r#"
        interface Slot<T, E> {
            value: T
        }
        class Pair<L, R> {
            left: L
            right: R
            implements Slot<L, R> {
                value as left
            }
            implements Slot<R, L> {
                value as right
            }
        }
        function main() -> bool {
            let p: Pair<int, string> = Pair { left: 7, right: "seven" }
            let lr: Slot<int, string> = p
            let rl: Slot<string, int> = p
            return lr.value == 7 && rl.value == "seven"
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

#[tokio::test]
async fn generic_interface_method_dispatch_preserves_swapped_type_var_identity_runtime() {
    let output = baml_test!(
        r#"
        interface Reporter<T, E> {
            function show(self) -> T
        }
        class Pair<L, R> {
            left: L
            right: R
            implements Reporter<L, R> {
                function show(self) -> L {
                    return self.left
                }
            }
            implements Reporter<R, L> {
                function show(self) -> R {
                    return self.right
                }
            }
        }
        function main() -> bool {
            let p: Pair<int, string> = Pair { left: 7, right: "seven" }
            let lr: Reporter<int, string> = p
            let rl: Reporter<string, int> = p
            return lr.show() == 7 && rl.show() == "seven"
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

#[tokio::test]
async fn interface_field_via_requires_chain_runtime() {
    let output = baml_test!(
        r#"
        interface Named { name: string }
        interface Aged { age: int }
        interface Person requires Named, Aged {
            occupation: string
        }
        class Employee {
            name: string
            age: int
            occupation: string
            salary: float
            implements Named {}
            implements Aged {}
            implements Person {}
        }
        function main() -> int {
            let e = Employee { name: "Dan", age: 35, occupation: "PM", salary: 1.0 }
            let a: Aged = e
            return a.age
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(35));
}

// ── Group M: default.method() ───────────────────────────────────────────────

#[tokio::test]
async fn default_call_from_override_returns_string() {
    let output = baml_test!(
        r#"
        interface Logger {
            function log(self, msg: string) -> string {
                return "[LOG] " + msg
            }
        }
        class TimestampLogger {
            prefix: string
            implements Logger {
                function log(self, msg: string) -> string {
                    return self.prefix + " " + default.log(msg)
                }
            }
        }
        function main() -> string {
            let tl = TimestampLogger { prefix: "X" }
            return tl.log("test")
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("X [LOG] test".into())
    );
}

#[tokio::test]
async fn default_resolves_to_current_block() {
    // Two interfaces each provide a default `tag`; an override in `A` must
    // see A's default, not B's.
    let output = baml_test!(
        r#"
        interface A {
            function tag(self) -> string { return "A" }
        }
        interface B {
            function tag(self) -> string { return "B" }
        }
        class X {
            implements A {
                function tag(self) -> string { return default.tag() + "!" }
            }
            implements B {}
        }
        function main() -> string {
            let x = X {}
            return x.as<A>.tag()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("A!".into())
    );
}

// ── Group N: `.as<I>` projections ───────────────────────────────────────────

#[tokio::test]
async fn as_projection_same_signature_runtime() {
    // BEP-044 §"Method Disambiguation": a class may declare `encode` in
    // two `implements` blocks. The class compiles; only unqualified
    // call sites would be ambiguous. Projections like
    // `h.as<Serializer>.encode()` select the target interface.
    let output = baml_test!(
        r#"
        interface Serializer {
            function encode(self) -> string
        }
        interface BinarySerializer {
            function encode(self) -> string
        }
        class Hybrid {
            implements Serializer {
                function encode(self) -> string { return "json:{}" }
            }
            implements BinarySerializer {
                function encode(self) -> string { return "binary" }
            }
        }
        function main() -> string {
            let h = Hybrid {}
            return h.as<Serializer>.encode()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("json:{}".into())
    );
}

#[tokio::test]
async fn as_projection_works_when_unambiguous_runtime() {
    let output = baml_test!(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        function main() -> string {
            let d = Dog {}
            return d.as<Animal>.speak()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Woof!".into())
    );
}

#[tokio::test]
async fn self_as_projection_call_inside_unrelated_block() {
    let output = baml_test!(
        r#"
        interface Greeter {
            function greet(self) -> string
        }
        interface Farewell {
            function bye(self) -> string
        }
        class Polite {
            name: string
            implements Greeter {
                function greet(self) -> string {
                    return "Hello, I'm " + self.name
                }
            }
            implements Farewell {
                function bye(self) -> string {
                    return self.as<Greeter>.greet() + " — and goodbye!"
                }
            }
        }
        function main() -> string {
            let p = Polite { name: "Alice" }
            return p.bye()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Hello, I'm Alice — and goodbye!".into())
    );
}

#[tokio::test]
async fn diamond_as_projection_call_runtime() {
    let output = baml_test!(
        r#"
        interface Base {
            function foo(self) -> string { return "Base" }
        }
        interface Left requires Base {
            function foo(self) -> string { return "Left" }
        }
        interface Right requires Base {
            function foo(self) -> string { return "Right" }
        }
        class D {
            implements Base {}
            implements Left {}
            implements Right {}
        }
        function main() -> string {
            let d = D {}
            return d.as<Left>.foo() + ":" + d.as<Right>.foo()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Left:Right".into())
    );
}

#[test]
fn old_interface_qualified_projection_is_compile_error() {
    assert_compile_error_contains(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        function bad() -> string {
            let d = Dog {}
            return d.Animal.speak()
        }
        "#,
        ".as<Animal>",
    );
}

// ── Group O: dispatch through interface-typed value ─────────────────────────

#[tokio::test]
async fn interface_typed_var_dispatches_to_concrete() {
    let output = baml_test!(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Cat {
            implements Animal {
                function speak(self) -> string { return "Meow." }
            }
        }
        function main() -> string {
            let a: Animal = Cat {}
            return a.speak()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Meow.".into())
    );
}

#[tokio::test]
async fn heterogeneous_interface_array_dispatches() {
    let output = baml_test!(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        class Cat {
            implements Animal {
                function speak(self) -> string { return "Meow." }
            }
        }
        function main() -> string {
            let animals: Animal[] = [Dog {}, Cat {}]
            return animals[0].speak() + animals[1].speak()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Woof!Meow.".into())
    );
}

#[tokio::test]
async fn cast_to_parent_interface_via_requires_runtime() {
    let output = baml_test!(
        r#"
        interface Named { name: string }
        interface Person requires Named {
            function introduce(self) -> string
        }
        class Employee {
            name: string
            implements Named {}
            implements Person {
                function introduce(self) -> string { return "hi" }
            }
        }
        function main() -> string {
            let e = Employee { name: "Alice" }
            let n: Named = e
            return n.name
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Alice".into())
    );
}

#[tokio::test]
async fn default_method_dispatch_through_interface_var() {
    let output = baml_test!(
        r#"
        interface Animal {
            name: string
            function speak(self) -> string
            function describe(self) -> string {
                return "animal: " + self.name
            }
        }
        class Dog {
            name: string
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        function main() -> string {
            let a: Animal = Dog { name: "Rex" }
            return a.describe()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("animal: Rex".into())
    );
}

#[tokio::test]
async fn interface_default_method_reference_accepts_explicit_receiver() {
    let output = baml_test!(
        r#"
        interface Describable {
            function describe(self) -> string {
                return "default"
            }
        }
        class Thing {
            implements Describable {}
        }
        function main() -> string {
            let t = Thing {}
            let describe = Describable.describe
            return describe(t)
        }
        "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("default".into())
    );
}

#[tokio::test]
async fn throwing_interface_dispatch_uses_active_catch() {
    let output = baml_test!(
        r#"
        class Boom {
            message: string
        }

        interface Fallible {
            function run(self) -> string throws Boom
        }

        class Worker {
            implements Fallible {
                function run(self) -> string throws Boom {
                    throw Boom { message: "caught" }
                }
            }
        }

        function main() -> string {
            let f: Fallible = Worker {}
            return f.run() catch (e) {
                let err: Boom => err.message
            }
        }
        "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("caught".into())
    );
}

#[tokio::test]
async fn override_dispatched_not_default() {
    let output = baml_test!(
        r#"
        interface Animal {
            name: string
            function describe(self) -> string {
                return "default:" + self.name
            }
        }
        class Cat {
            name: string
            implements Animal {
                function describe(self) -> string {
                    return "cat:" + self.name
                }
            }
        }
        function main() -> string {
            let a: Animal = Cat { name: "Luna" }
            return a.describe()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("cat:Luna".into())
    );
}

// ── Group P: match narrowing ────────────────────────────────────────────────

#[tokio::test]
async fn match_narrows_to_concrete_field_access() {
    let output = baml_test!(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            breed: string
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        function main() -> string {
            let a: Animal = Dog { breed: "Lab" }
            return match (a) {
                let d: Dog => d.breed
                _ => "other"
            }
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Lab".into())
    );
}

#[tokio::test]
async fn match_destructures_interface_fields() {
    let output = baml_test!(
        r#"
        interface Animal {
            name: string
            function speak(self) -> string
        }
        class Dog {
            name: string
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        function main() -> string {
            let a: Animal = Dog { name: "Rex" }
            return match (a) {
                let d: Dog => d.name
                _ => "other"
            }
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Rex".into())
    );
}

#[tokio::test]
async fn match_open_interface_with_wildcard_works() {
    let output = baml_test!(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        class Duck {
            implements Animal {
                function speak(self) -> string { return "Quack!" }
            }
        }
        function main() -> string {
            let a: Animal = Duck {}
            return match (a) {
                let d: Dog => "dog"
                _ => "other"
            }
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("other".into())
    );
}

// ── Group Q: generics ───────────────────────────────────────────────────────

#[tokio::test]
async fn generic_interface_concrete_type_param_runtime() {
    let output = baml_test!(
        r#"
        interface Container<T> {
            function add(self, item: T) -> null
            function get(self, index: int) -> T?
            function size(self) -> int
        }
        class IntStack {
            items: int[]
            implements Container<int> {
                function add(self, item: int) -> null {
                    self.items.push(item)
                    return null
                }
                function get(self, index: int) -> int? {
                    return self.items[index]
                }
                function size(self) -> int {
                    return self.items.length()
                }
            }
        }
        function main() -> int {
            let s = IntStack { items: [] }
            s.add(10)
            s.add(20)
            return s.size()
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(2));
}

#[tokio::test]
async fn same_generic_interface_different_type_params_disambiguated_with_as_projection() {
    let output = baml_test!(
        r#"
        interface Converter<T> {
            function convert(self) -> T
        }
        class MultiFormat {
            data: string
            implements Converter<int> {
                function convert(self) -> int { return 42 }
            }
            implements Converter<float> {
                function convert(self) -> float { return 42.5 }
            }
        }
        function read_int<T extends Converter<int>>(m: T) -> int {
            return m.as<Converter<int>>.convert()
        }
        function read_float<T extends Converter<float>>(m: T) -> float {
            return m.as<Converter<float>>.convert()
        }
        function main() -> bool {
            let m = MultiFormat { data: "payload" }
            return read_int<MultiFormat>(m) == 42
                && read_float<MultiFormat>(m) == 42.5
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

#[tokio::test]
async fn generic_bound_runtime() {
    // Interface-typed arrays preserve the interface field view and dispatch
    // through the concrete implementor.
    let output = baml_test!(
        r#"
        interface Named { name: string }
        class Dog {
            name: string
            implements Named {}
        }
        function first_name(items: Named[]) -> string {
            return items[0].name
        }
        function main() -> string {
            let mydog = Dog { name: "Rex" }
            mydog.name // works
            let dogs: Named[] = [Dog { name: "Rex" }, Dog { name: "Buddy" }]
            return first_name(dogs)
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Rex".into())
    );
}

// ── Group R: reflection ─────────────────────────────────────────────────────

#[tokio::test]
async fn reflect_type_of_interface_to_string() {
    let output = baml_test!(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        function main() -> string {
            return reflect.type_of<Animal>().to_string()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Animal".into())
    );
}

#[tokio::test]
async fn reflect_implements_true_for_implementor() {
    let output = baml_test!(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        function main() -> bool {
            return reflect.type_of<Dog>().implements(reflect.type_of<Animal>())
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

#[tokio::test]
async fn reflect_implements_false_for_non_implementor() {
    let output = baml_test!(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Rock {
            mass: int
        }
        function main() -> bool {
            return reflect.type_of<Rock>().implements(reflect.type_of<Animal>())
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(false));
}

#[tokio::test]
async fn reflect_implemented_by_inverse_runtime() {
    let output = baml_test!(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        function main() -> bool {
            let direct = reflect.type_of<Dog>().implements(reflect.type_of<Animal>())
            let reverse = reflect.type_of<Animal>().implemented_by(reflect.type_of<Dog>())
            return direct == reverse
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

#[tokio::test]
async fn reflect_implements_transitive_via_requires() {
    let output = baml_test!(
        r#"
        interface Named { name: string }
        interface Person requires Named {
            function introduce(self) -> string
        }
        class Employee {
            name: string
            implements Named {}
            implements Person {
                function introduce(self) -> string { return "hi" }
            }
        }
        function main() -> bool {
            return reflect.type_of<Employee>().implements(reflect.type_of<Named>())
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

#[tokio::test]
async fn reflect_implementors_lists_lexicographic_order_and_identity() {
    // `implementors()` returns implementors in a deterministic lexicographic
    // order by qualified name — `Cat` before `Dog` — independent of source
    // declaration order.
    let output = baml_test!(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        class Cat {
            implements Animal {
                function speak(self) -> string { return "Meow." }
            }
        }
        function main() -> bool {
            let impls = reflect.type_of<Animal>().implementors()
            return impls.length() == 2
                && impls[0] == reflect.type_of<Cat>()
                && impls[1] == reflect.type_of<Dog>()
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

#[tokio::test]
async fn reflect_implementors_empty_for_concrete_class() {
    let output = baml_test!(
        r#"
        class Dog {
            breed: string
        }
        function main() -> int {
            return reflect.type_of<Dog>().implementors().length()
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(0));
}

#[tokio::test]
async fn reflect_implementors_empty_for_primitive() {
    let output = baml_test!(
        r#"
        function main() -> int {
            return reflect.type_of<int>().implementors().length()
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(0));
}

#[tokio::test]
async fn reflect_implements_inside_generic_function() {
    let output = baml_test!(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        function is_animal<T>() -> bool {
            return reflect.type_of<T>().implements(reflect.type_of<Animal>())
        }
        function main() -> bool {
            return is_animal<Dog>() && is_animal<int>() == false
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

#[tokio::test]
async fn reflect_interface_does_not_implement_itself() {
    // BEP-044: an interface never implements itself.
    let output = baml_test!(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        function main() -> bool {
            return reflect.type_of<Animal>().implements(reflect.type_of<Animal>())
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(false));
}

// ── Group S: class methods + interface coexist ──────────────────────────────

#[tokio::test]
async fn class_own_method_callable_from_implements_block() {
    let output = baml_test!(
        r#"
        interface Configurable {
            function configure(self) -> string
        }
        class Server {
            host: string

            function address(self) -> string {
                return self.host
            }

            implements Configurable {
                function configure(self) -> string {
                    return self.address()
                }
            }
        }
        function main() -> string {
            let s = Server { host: "localhost" }
            return s.configure()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("localhost".into())
    );
}

#[tokio::test]
async fn _unused_imports_compile() {
    // Silence dead-code warnings for `Ty` if all runtime tests above eventually
    // get gated/removed. Touching it here keeps the import live.
    let _ = Ty::string();
}

// ─────────────────────────────────────────────────────────────────────────────
// Additional BEP-044 coverage for generic bounds, `Self`, interface
// conversions, out-of-body implementations, and subtyping edge cases.
// ─────────────────────────────────────────────────────────────────────────────

// ── Group U: generic bounds (parser + bound enforcement) ────────────────────

#[tokio::test]
async fn generic_bound_extends_named_runtime() {
    // BEP-044 spec: `<T extends Named>` constrains `T` to types that
    // implement `Named`, exposing `name` on values of type `T`.
    let output = baml_test!(
        r#"
        interface Named { name: string }
        class Dog {
            name: string
            implements Named {}
        }
        function first_name<T extends Named>(items: T[]) -> string {
            return items[0].name
        }
        function main() -> string {
            let dogs: Dog[] = [Dog { name: "Rex" }, Dog { name: "Buddy" }]
            return first_name<Dog>(dogs)
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Rex".into())
    );
}

#[test]
fn generic_bound_violation_is_compile_error() {
    // Calling `<T extends Named>` with an unrelated `T` should fail at
    // compile time once bound enforcement lands.
    assert_compile_error_contains(
        r#"
        interface Named { name: string }
        function first_name<T extends Named>(items: T[]) -> string {
            return items[0].name
        }
        function main() -> string {
            let xs: int[] = [1, 2, 3]
            return first_name<int>(xs)
        }
        "#,
        "Named",
    );
}

#[test]
fn generic_bound_alias_syntax_is_compile_error() {
    assert_compile_error_contains(
        r#"
        interface Converter<T> {
            function convert(self) -> T
        }
        function read_int<T extends Converter<int> as Ints>(m: T) -> int {
            return m.as<Converter<int>>.convert()
        }
        "#,
        "generic bound aliases are not supported",
    );
}

#[tokio::test]
async fn generic_bound_as_projection_selects_generic_interface_instantiation() {
    let output = baml_test!(
        r#"
        interface Converter<T> {
            function convert(self) -> T
        }
        class MultiFormat {
            data: string
            implements Converter<int> {
                function convert(self) -> int { return 42 }
            }
            implements Converter<float> {
                function convert(self) -> float { return 42.5 }
            }
        }
        function read_int<T extends Converter<int>>(m: T) -> int {
            return m.as<Converter<int>>.convert()
        }
        function main() -> int {
            return read_int<MultiFormat>(MultiFormat { data: "payload" })
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(42));
}

#[tokio::test]
async fn generic_bound_direct_method_call_dispatches_through_interface() {
    let output = baml_test!(
        r#"
        interface Converter<T> {
            function convert(self) -> T
        }
        class IntBox {
            value: int
            implements Converter<int> {
                function convert(self) -> int { return self.value }
            }
        }
        function read_int<T extends Converter<int>>(m: T) -> int {
            return m.convert()
        }
        function main() -> int {
            return read_int<IntBox>(IntBox { value: 42 })
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(42));
}

#[test]
fn same_interface_different_type_args_is_not_assignable() {
    assert_compile_error_contains(
        r#"
        interface Box<T> {
            function get(self) -> T
        }
        function bad(x: Box<int>) -> Box<string> {
            return x
        }
        "#,
        "type mismatch",
    );
}

#[tokio::test]
async fn generic_interface_method_preserves_method_type_param() {
    let output = baml_test!(
        r#"
        interface Echo<T> {
            function echo<U>(self, value: U) -> U
        }
        class Echoer {
            implements Echo<int> {
                function echo<U>(self, value: U) -> U {
                    return value
                }
            }
        }
        function main() -> string {
            let e: Echo<int> = Echoer {}
            return e.echo<string>("ok")
        }
        "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("ok".into())
    );
}

#[test]
fn generic_interface_method_explicit_type_args_are_checked() {
    assert_compile_error_contains(
        r#"
        interface Echo<T> {
            function echo<U>(self, value: U) -> U
        }
        class Echoer {
            implements Echo<int> {
                function echo<U>(self, value: U) -> U {
                    return value
                }
            }
        }
        function bad() -> string {
            let e: Echo<int> = Echoer {}
            return e.echo<int>("nope")
        }
        "#,
        "type mismatch",
    );
}

#[test]
fn required_interface_method_generic_bound_mismatch_is_error() {
    assert_compile_error_code(
        r#"
        interface Named { name: string }
        interface Reader {
            function read<T>(self, value: T) -> string
        }
        class ReaderImpl {
            implements Reader {
                function read<T extends Named>(self, value: T) -> string {
                    return value.name
                }
            }
        }
        "#,
        "E0120",
    );
}

#[test]
fn generic_bounds_accept_compound_type_expressions() {
    assert_zero_compile_errors(
        r#"
        function keep_union<T extends int | string>(x: T) -> int {
            return 1
        }
        function len_list<T extends int[]>(xs: T) -> int {
            return 1
        }
        function keep_optional<T extends string?>(x: T) -> int {
            return 1
        }
        function main() -> int {
            return keep_union<int>(1) + len_list<int[]>([1, 2, 3])
        }
        "#,
    );
}

#[tokio::test]
async fn generic_bound_through_extends_chain() {
    // `Person requires Named` — `<T extends Person>` should also expose
    // `Named`'s `name`.
    let output = baml_test!(
        r#"
        interface Named { name: string }
        interface Person requires Named {
            occupation: string
        }
        class Employee {
            name: string
            occupation: string
            implements Named {}
            implements Person {}
        }
        function greet<T extends Person>(p: T) -> string {
            return "Hi, " + p.name + " the " + p.occupation
        }
        function main() -> string {
            let e = Employee { name: "Alice", occupation: "PM" }
            return greet<Employee>(e)
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Hi, Alice the PM".into())
    );
}

// ── Group W: LLM oneOf rendering for interface return types ─────────────────
//
// The BEP states that an LLM function returning an interface should
// render every implementor in the prompt's `oneOf` schema. We exercise
// the type-level guarantee — the program compiles and the function's
// return type is correct — but pin the schema-shape requirement in a
// follow-up snapshot harness once it exists.

#[test]
fn llm_function_can_return_interface_type() {
    assert_no_interface_errors(
        r##"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        class Cat {
            implements Animal {
                function speak(self) -> string { return "Meow." }
            }
        }
        function detect_animal(description: string) -> Animal {
            client GPT4o
            prompt #"
                Identify the animal from the description: {{description}}.
                {{ ctx.output_format }}
            "#
        }
        "##,
    );
}

#[test]
fn llm_function_returning_interface_enumerates_implementors_in_schema() {
    // BEP-044 §"LLM Functions": a function declared to return an
    // interface must compile, with the schema-rendering side later
    // expanding the interface into a `oneOf` of its implementors at
    // prompt evaluation time. This test pins the type-check surface;
    // the prompt-rendering snapshot would require a separate harness
    // that captures the rendered Jinja output.
    assert_no_interface_errors(
        r##"
        client<llm> GPT4o {
            provider openai
            options { model "gpt-4o" }
        }
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        class Cat {
            implements Animal {
                function speak(self) -> string { return "Meow." }
            }
        }
        function detect_animal(description: string) -> Animal {
            client GPT4o
            prompt #"
                Identify the animal: {{ description }}.
                {{ ctx.output_format }}
            "#
        }
        "##,
    );
}

// ── Group Y: Self return / param types (BEP-044 deferred) ───────────────────

#[tokio::test]
async fn self_return_type_carries_concrete_class() {
    // `function clone(self) -> Self` should preserve the concrete class
    // through the return so callers don't need to up-cast.
    let output = baml_test!(
        r#"
        interface Cloneable {
            function clone(self) -> Self
        }
        class Box {
            value: int
            implements Cloneable {
                function clone(self) -> Self {
                    return Box { value: self.value }
                }
            }
        }
        function main() -> int {
            let b = Box { value: 42 }
            let c = b.clone()
            return c.value
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(42));
}

#[tokio::test]
async fn self_return_on_interface_typed_receiver_collapses_to_interface() {
    let output = baml_test!(
        r#"
        interface Cloneable {
            value: int
            function clone(self) -> Self
        }
        class Box {
            value: int
            implements Cloneable {
                function clone(self) -> Self {
                    return Box { value: self.value }
                }
            }
        }
        function main() -> int {
            let c: Cloneable = Box { value: 42 }
            let cloned = c.clone()
            return cloned.value
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(42));
}

#[tokio::test]
async fn multi_self_method_accepts_concrete_receiver() {
    let output = baml_test!(
        r#"
        interface Equatable {
            function same(self, other: Self) -> bool
        }
        class Box {
            value: int
            implements Equatable {
                function same(self, other: Self) -> bool {
                    return self.value == other.value
                }
            }
        }
        function main() -> bool {
            let a = Box { value: 7 }
            let b = Box { value: 7 }
            return a.same(b)
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

#[test]
fn multi_self_method_rejected_on_interface_typed_receiver() {
    assert_compile_error_contains(
        r#"
        interface Equatable {
            function same(self, other: Self) -> bool
        }
        class Box {
            value: int
            implements Equatable {
                function same(self, other: Self) -> bool {
                    return self.value == other.value
                }
            }
        }
        function bad(a: Equatable, b: Equatable) -> bool {
            return a.same(b)
        }
        "#,
        "concrete receiver",
    );
}

#[test]
fn default_method_returning_self_is_compile_error() {
    assert_compile_error_contains(
        r#"
        interface Cloneable {
            function clone(self) -> Self {
                return self
            }
        }
        class Box {
            value: int
            implements Cloneable {
                function clone(self) -> Self {
                    return Box { value: self.value }
                }
            }
        }
        "#,
        "Self",
    );
}

#[test]
fn self_type_outside_interface_or_class_is_compile_error() {
    assert_compile_error_contains(
        r#"
        function bad(x: Self) -> Self {
            return x
        }
        "#,
        "Self",
    );
}

// ── Group Z: interface-to-interface casting (BEP-044) ────────────────────────

#[test]
fn cast_from_one_interface_to_another_when_class_implements_both_is_error() {
    // BEP-044: two unrelated interfaces are NOT subtypes even if a class
    // implements both. Must narrow via `match`/`is` first.
    assert_compile_error_contains(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        interface Swimmer {
            function swim(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
            implements Swimmer {
                function swim(self) -> string { return "splash" }
            }
        }
        function main() -> string {
            let a: Animal = Dog {}
            let s: Swimmer = a
            return s.swim()
        }
    "#,
        "Animal",
    );
}

#[tokio::test]
async fn interface_to_interface_conversion_via_unknown_match_narrowing() {
    // BEP-044 §"Converting Between Interface Types": cross-interface
    // conversion is explicit. First erase to `unknown`, then narrow to the
    // concrete implementor, then assign the narrowed value to the other
    // interface.
    let output = baml_test!(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        interface Swimmer {
            function swim(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
            implements Swimmer {
                function swim(self) -> string { return "splash" }
            }
        }
        function main() -> string {
            let a: Animal = Dog {}
            let value: unknown = a
            return match (value) {
                let d: Dog => {
                    let s: Swimmer = d
                    s.swim()
                }
                _ => "not a swimmer"
            }
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("splash".into())
    );
}

#[tokio::test]
async fn as_upcast_selects_interface_field_link_runtime() {
    let output = baml_test!(
        r#"
        interface Named { name: string }
        interface Labeled { name: string }
        class Item {
            named_name: string
            labeled_name: string
            implements Named { name as named_name }
            implements Labeled { name as labeled_name }
        }
        function main() -> bool {
            let i = Item { named_name: "widget", labeled_name: "WIDGET-001" }
            return i.as<Named>.name == "widget"
                && i.as<Labeled>.name == "WIDGET-001"
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

#[test]
fn as_rejects_interface_downcast() {
    assert_compile_error_contains(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        interface Swimmer {
            function swim(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
            implements Swimmer {
                function swim(self) -> string { return "splash" }
            }
        }
        function bad(a: Animal) -> string {
            let s = a.as<Swimmer>
            return s.swim()
        }
        "#,
        "Animal",
    );
}

#[test]
fn as_requires_interface_target() {
    assert_compile_error_contains(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        function bad(d: Dog) -> Dog {
            return d.as<Dog>
        }
        "#,
        "requires an interface target",
    );
}

// ── Group AA: dispatch edge cases — non-local receivers ─────────────────────

#[tokio::test]
async fn dispatch_through_function_call_result() {
    // `f().speak()` where `f()` returns an interface-typed value should
    // dispatch dynamically. Today the MIR intercepts only direct local
    // receivers, so the type-tag switch isn't emitted here.
    let output = baml_test!(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Cat {
            implements Animal {
                function speak(self) -> string { return "Meow." }
            }
        }
        function get_animal() -> Animal {
            return Cat {}
        }
        function main() -> string {
            return get_animal().speak()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Meow.".into())
    );
}

#[tokio::test]
async fn dispatch_through_array_index_with_field_access() {
    let output = baml_test!(
        r#"
        interface Animal {
            name: string
            function speak(self) -> string
        }
        class Cat {
            name: string
            implements Animal {
                function speak(self) -> string { return "Meow." }
            }
        }
        function main() -> string {
            let animals: Animal[] = [Cat { name: "Luna" }]
            return animals[0].name
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Luna".into())
    );
}

#[tokio::test]
async fn dispatch_through_field_access_chain() {
    // `wrapper.animal.speak()` — receiver is a field access, not a
    // local. Path interception needs to recognize this longer chain.
    let output = baml_test!(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Cat {
            implements Animal {
                function speak(self) -> string { return "Meow." }
            }
        }
        class Wrapper {
            animal: Animal
        }
        function main() -> string {
            let w = Wrapper { animal: Cat {} }
            return w.animal.speak()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Meow.".into())
    );
}

// ── Group AB: diamond + multi-level requires ────────────────────────────────

#[tokio::test]
async fn requires_chain_four_levels_deep() {
    let output = baml_test!(
        r#"
        interface A { function tag(self) -> string }
        interface B requires A {}
        interface C requires B {}
        interface D requires C {}
        class Leaf {
            implements A { function tag(self) -> string { return "leaf" } }
            implements B {}
            implements C {}
            implements D {}
        }
        function main() -> string {
            let l: A = Leaf {}
            return l.tag()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("leaf".into())
    );
}

#[tokio::test]
async fn diamond_call_through_interface_typed_var() {
    // Through an interface-typed variable the dispatch is unambiguous —
    // the static type picks the vtable. No qualifier needed.
    let output = baml_test!(
        r#"
        interface Base { function foo(self) -> string { return "Base" } }
        interface Left requires Base { function foo(self) -> string { return "Left" } }
        interface Right requires Base { function foo(self) -> string { return "Right" } }
        class D {
            implements Base {}
            implements Left {}
            implements Right {}
        }
        function main() -> string {
            let l: Left = D {}
            let r: Right = D {}
            return l.foo() + ":" + r.foo()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Left:Right".into())
    );
}

// ── Group AC: `default` keyword corner cases ────────────────────────────────

#[test]
fn default_keyword_outside_implements_block_is_compile_error() {
    // `default` only resolves inside an `implements` block body. A free
    // function shouldn't see it as a magic identifier.
    assert_compile_error_contains(
        r#"
        interface Logger {
            function log(self, msg: string) -> string { return msg }
        }
        function rogue(msg: string) -> string {
            return default.log(msg)
        }
        "#,
        "default",
    );
}

#[test]
fn default_keyword_inside_class_method_outside_implements_is_compile_error() {
    // A class-level (non-implements-block) method also has no `default`.
    assert_compile_error_contains(
        r#"
        interface Logger {
            function log(self, msg: string) -> string { return msg }
        }
        class Helper {
            function rogue(self, msg: string) -> string {
                return default.log(msg)
            }
        }
        "#,
        "default",
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Stress tests — exercise working surface in non-trivial shapes
// ─────────────────────────────────────────────────────────────────────────────

// ── Group AD: dispatch through chained method calls ─────────────────────────

#[tokio::test]
async fn dispatch_chained_three_function_calls_returning_interfaces() {
    // Three levels of `f().g().h()` where each returns an interface,
    // and the final `.speak()` dispatches through the type-tag switch.
    let output = baml_test!(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Cat {
            implements Animal {
                function speak(self) -> string { return "Meow." }
            }
        }
        function fresh() -> Animal {
            return Cat {}
        }
        function rebox(a: Animal) -> Animal {
            return a
        }
        function main() -> string {
            return rebox(rebox(fresh())).speak()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Meow.".into())
    );
}

#[tokio::test]
async fn dispatch_chained_methods_returning_interface_each_level() {
    // Each link returns Animal so we can chain `.next().next()...` then
    // dispatch `.speak()` through the type-tag switch on the final
    // result. `next` lives on the interface so it's callable on every
    // intermediate value.
    let output = baml_test!(
        r#"
        interface Animal {
            function speak(self) -> string
            function next(self) -> Animal { return self }
        }
        class Cat {
            implements Animal {
                function speak(self) -> string { return "Meow." }
            }
        }
        function main() -> string {
            let c = Cat {}
            let a: Animal = c
            return a.next().next().next().speak()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Meow.".into())
    );
}

// ── Group AE: dispatch through deeply nested field/array/index chains ───────

#[tokio::test]
async fn dispatch_through_nested_array_of_arrays() {
    // `pack[0][1].speak()` — index twice into an `Animal[][]`.
    let output = baml_test!(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        class Cat {
            implements Animal {
                function speak(self) -> string { return "Meow." }
            }
        }
        function main() -> string {
            let pack: Animal[][] = [
                [Dog {}, Cat {}],
                [Cat {}, Dog {}],
            ]
            return pack[0][1].speak() + ":" + pack[1][0].speak()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Meow.:Meow.".into())
    );
}

#[tokio::test]
async fn dispatch_through_field_of_array_of_interface() {
    // `wrapper.pets[1].speak()` — field then array index then dispatch.
    let output = baml_test!(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        class Cat {
            implements Animal {
                function speak(self) -> string { return "Meow." }
            }
        }
        class Wrapper {
            pets: Animal[]
        }
        function main() -> string {
            let w = Wrapper { pets: [Dog {}, Cat {}] }
            return w.pets[1].speak()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Meow.".into())
    );
}

#[tokio::test]
async fn dispatch_through_five_level_field_chain() {
    // Root.a.b.c.d.e.speak() — five field accesses then dispatch.
    let output = baml_test!(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Cat {
            implements Animal {
                function speak(self) -> string { return "Meow." }
            }
        }
        class L1 { e: Animal }
        class L2 { d: L1 }
        class L3 { c: L2 }
        class L4 { b: L3 }
        class L5 { a: L4 }
        function main() -> string {
            let r = L5 { a: L4 { b: L3 { c: L2 { d: L1 { e: Cat {} } } } } }
            return r.a.b.c.d.e.speak()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Meow.".into())
    );
}

#[tokio::test]
async fn dispatch_through_map_value_field_then_method() {
    // `directory["key"].speak()` — map indexed by string, value is
    // interface-typed.
    let output = baml_test!(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Cat {
            implements Animal {
                function speak(self) -> string { return "Meow." }
            }
        }
        function main() -> string {
            let directory: map<string, Animal> = { "luna": Cat {} }
            return directory["luna"].speak()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Meow.".into())
    );
}

// ── Group AF: dispatch combined with control-flow producing interfaces ──────

#[tokio::test]
async fn dispatch_through_branch_assigned_interface() {
    // The interface-typed local is initialised from one of two branches.
    // The dispatch must see the right runtime class regardless of which
    // branch ran.
    let output = baml_test!(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        class Cat {
            implements Animal {
                function speak(self) -> string { return "Meow." }
            }
        }
        function pick(use_dog: bool) -> Animal {
            if (use_dog) {
                return Dog {}
            } else {
                return Cat {}
            }
        }
        function main() -> string {
            return pick(true).speak() + ":" + pick(false).speak()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Woof!:Meow.".into())
    );
}

#[tokio::test]
async fn dispatch_for_loop_over_interface_array_collects_results() {
    // Iterate over an interface-typed array, accumulating each
    // implementor's response. Verifies dispatch fires for every
    // iteration variable.
    let output = baml_test!(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        class Cat {
            implements Animal {
                function speak(self) -> string { return "Meow." }
            }
        }
        class Duck {
            implements Animal {
                function speak(self) -> string { return "Quack!" }
            }
        }
        function main() -> string {
            let zoo: Animal[] = [Dog {}, Cat {}, Duck {}, Cat {}, Dog {}]
            let acc: string = ""
            for (let a in zoo) {
                acc = acc + a.speak() + ","
            }
            return acc
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Woof!,Meow.,Quack!,Meow.,Woof!,".into())
    );
}

// ── Group AG: diamond / requires — deeper trees ─────────────────────────────

#[tokio::test]
async fn diamond_six_level_requires_chain() {
    // Six-level chain A < B < C < D < E < F. The leaf class implements
    // F; calls through any ancestor interface should dispatch to the
    // leaf's method.
    let output = baml_test!(
        r#"
        interface A { function tag(self) -> string }
        interface B requires A {}
        interface C requires B {}
        interface D requires C {}
        interface E requires D {}
        interface F requires E {}
        class Deep {
            implements A { function tag(self) -> string { return "deep" } }
            implements B {}
            implements C {}
            implements D {}
            implements E {}
            implements F {}
        }
        function main() -> string {
            let a: A = Deep {}
            let b: B = Deep {}
            let c: C = Deep {}
            let d: D = Deep {}
            let e: E = Deep {}
            let f: F = Deep {}
            return a.tag() + b.tag() + c.tag() + d.tag() + e.tag() + f.tag()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("deepdeepdeepdeepdeepdeep".into())
    );
}

#[tokio::test]
async fn diamond_with_overrides_at_each_level() {
    // Each interface in the chain declares its own default `name`. Per
    // BEP-044 §"Method Disambiguation", dispatch through an interface-
    // typed variable selects that interface's vtable: the static type of
    // the receiver picks which level's default runs. So `b: Base` reads
    // `"Base"`, `m: Mid` reads `"Mid"`, and `t: Tip` reads `"Tip"`.
    let output = baml_test!(
        r#"
        interface Base { function name(self) -> string { return "Base" } }
        interface Mid requires Base { function name(self) -> string { return "Mid" } }
        interface Tip requires Mid { function name(self) -> string { return "Tip" } }
        class Concrete {
            implements Base {}
            implements Mid {}
            implements Tip {}
        }
        function main() -> string {
            let b: Base = Concrete {}
            let m: Mid = Concrete {}
            let t: Tip = Concrete {}
            return b.name() + ":" + m.name() + ":" + t.name()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Base:Mid:Tip".into())
    );
}

#[tokio::test]
async fn double_diamond_two_independent_inheritance_paths() {
    // Two independent inheritance trees joined at the leaf class. Each
    // leaf interface uses a distinct method name to satisfy the project's
    // hard ambiguity rule, and the cross-tree composition still works.
    //
    //   BaseTag ─→ Left   (declares left_tag)
    //   BaseTag ─→ Right  (declares right_tag)
    //   BaseNote ─→ X     (declares x_note)
    //   BaseNote ─→ Y     (declares y_note)
    let output = baml_test!(
        r#"
        interface BaseTag {}
        interface Left requires BaseTag { function left_tag(self) -> string }
        interface Right requires BaseTag { function right_tag(self) -> string }
        interface BaseNote {}
        interface X requires BaseNote { function x_note(self) -> string }
        interface Y requires BaseNote { function y_note(self) -> string }
        class Hub {
            implements BaseTag {}
            implements Left {
                function left_tag(self) -> string { return "L" }
            }
            implements Right {
                function right_tag(self) -> string { return "R" }
            }
            implements BaseNote {}
            implements X {
                function x_note(self) -> string { return "X" }
            }
            implements Y {
                function y_note(self) -> string { return "Y" }
            }
        }
        function main() -> string {
            let h = Hub {}
            return h.left_tag() + h.right_tag() + h.x_note() + h.y_note()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("LRXY".into())
    );
}

// ── Group AH: `default` keyword corner cases ────────────────────────────────

#[tokio::test]
async fn default_called_twice_in_one_override_body() {
    // Two `default` calls in the same body — neither should infinitely
    // recurse and both should reach the interface's implementation.
    let output = baml_test!(
        r#"
        interface Logger {
            function log(self, msg: string) -> string {
                return "[L] " + msg
            }
        }
        class DoubleLogger {
            prefix: string
            implements Logger {
                function log(self, msg: string) -> string {
                    return self.prefix + default.log(msg) + " " + default.log("END")
                }
            }
        }
        function main() -> string {
            let dl = DoubleLogger { prefix: "PRE-" }
            return dl.log("hi")
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("PRE-[L] hi [L] END".into())
    );
}

#[tokio::test]
async fn default_call_with_computed_argument_expression() {
    // The argument to `default.X(...)` is a non-trivial expression that
    // touches `self` and string concatenation. Verifies argument
    // lowering happens before the static dispatch is wired up.
    let output = baml_test!(
        r#"
        interface Wrapper {
            function decorate(self, body: string) -> string {
                return "[" + body + "]"
            }
        }
        class Stamp {
            mark: string
            implements Wrapper {
                function decorate(self, body: string) -> string {
                    return default.decorate(self.mark + ":" + body)
                }
            }
        }
        function main() -> string {
            let s = Stamp { mark: "OK" }
            return s.decorate("hi")
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("[OK:hi]".into())
    );
}

#[test]
fn default_keyword_inside_lambda_inside_implements_block() {
    // `default.log(msg)` referenced from inside a lambda nested in the
    // override. Lambdas inherit access to `self` but the BEP-044 keyword
    // scoping rules don't reach this far yet, so this is a compile-time
    // failure today. Pin it and flip to a runtime check when lambdas
    // start tracking the enclosing implements-block context.
    assert_compile_error_contains(
        r#"
        interface Logger {
            function log(self, msg: string) -> string { return msg }
        }
        class L {
            implements Logger {
                function log(self, msg: string) -> string {
                    let f = (m: string) => default.log(m)
                    return f(msg)
                }
            }
        }
        "#,
        "default",
    );
}

#[tokio::test]
async fn default_keyword_shadowed_by_local_variable_resolves_to_local() {
    // A local `let default = ...` inside the override should win over
    // the BEP-044 magic identifier — `default.length()` then operates on
    // the local string, not the interface's default body.
    let output = baml_test!(
        r#"
        interface Logger {
            function log(self, msg: string) -> string { return msg }
        }
        class L {
            implements Logger {
                function log(self, msg: string) -> string {
                    let default: string = "shadow"
                    return default
                }
            }
        }
        function main() -> string {
            let l = L {}
            return l.log("ignored")
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("shadow".into())
    );
}

// ── Group AI: LLM-context interface composition ─────────────────────────────

#[test]
fn llm_function_with_interface_array_return_compiles() {
    // Returning `Animal[]` from an LLM function should also compile —
    // the schema generator must accept interface types in container
    // positions, not just as the bare top-level return.
    assert_no_interface_errors(
        r##"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        function detect_zoo(description: string) -> Animal[] {
            client GPT4o
            prompt #"
                Identify every animal mentioned in {{description}}.
                {{ ctx.output_format }}
            "#
        }
        "##,
    );
}

#[test]
fn llm_function_with_interface_in_union_return_compiles() {
    // An LLM function returning `Animal | string` should also be valid
    // — the type-checker must accept interfaces inside union positions.
    assert_no_interface_errors(
        r##"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        function detect_or_describe(description: string) -> Animal | string {
            client GPT4o
            prompt #"
                If {{description}} clearly identifies an animal, return one.
                Otherwise, paraphrase the description.
                {{ ctx.output_format }}
            "#
        }
        "##,
    );
}

#[test]
fn llm_function_takes_interface_typed_parameter_compiles() {
    // Interface types should be allowed as parameter types, with the
    // serializer rendering them as the underlying class shape. Pin the
    // type-checking surface; runtime semantics (concrete-class
    // serialization) are exercised by the LLM harness.
    assert_no_interface_errors(
        r##"
        interface Animal {
            name: string
            function speak(self) -> string
        }
        class Dog {
            name: string
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        function describe_animal(a: Animal) -> string {
            client GPT4o
            prompt #"
                Describe the animal named {{a.name}}.
                {{ ctx.output_format }}
            "#
        }
        "##,
    );
}

// ── Group AH: interface fields are class-owned ───────────────────────────────

fn assert_zero_compile_errors(source: &str) {
    let errors = collect_compile_errors(source);
    assert!(
        errors.is_empty(),
        "expected zero compile errors, got:\n  {}",
        errors.join("\n  ")
    );
}

#[test]
fn interface_fields_auto_link_from_class_fields() {
    assert_zero_compile_errors(
        r#"
        interface Config {
            host: string
            port: int
        }
        class Server {
            host: string
            port: int
            max_connections: int
            implements Config {}
        }
        "#,
    );
}

#[test]
fn interface_fields_auto_link_with_method() {
    assert_zero_compile_errors(
        r#"
        interface Animal {
            name: string
            age: int
            function speak(self) -> string
        }
        class Dog {
            name: string
            age: int
            breed: string
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        "#,
    );
}

#[test]
fn interface_field_links_can_surround_methods() {
    assert_zero_compile_errors(
        r#"
        interface Widget {
            id: string
            function render(self) -> string
            label: string
        }
        class Button {
            button_id: string
            text: string
            implements Widget {
                id as button_id
                function render(self) -> string { return "<button>" }
                label as text
            }
        }
        "#,
    );
}

#[test]
fn missing_class_field_for_interface_with_partial_links_is_error() {
    assert_compile_error_code(
        r#"
        interface Config {
            host: string
            port: int
        }
        class Server {
            host: string
            implements Config {
                host as host
            }
        }
        "#,
        "E0124",
    );
}

#[test]
fn declaring_field_in_implements_block_is_error() {
    assert_compile_error_code(
        r#"
        interface Config {
            port: int
        }
        class Server {
            implements Config {
                port: string
            }
        }
        "#,
        "E0127",
    );
}

#[test]
fn unknown_interface_field_in_link_is_error() {
    assert_compile_error_code(
        r#"
        interface Named {
            name: string
        }
        class Person {
            name: string
            implements Named {
                title as name
            }
        }
        "#,
        "E0128",
    );
}

#[test]
fn unknown_class_field_in_link_is_error() {
    assert_compile_error_code(
        r#"
        interface Named {
            name: string
        }
        class Person {
            name: string
            implements Named {
                name as display_name
            }
        }
        "#,
        "E0129",
    );
}

#[test]
fn duplicate_interface_field_link_is_error() {
    assert_compile_error_code(
        r#"
        interface Named {
            name: string
        }
        class Person {
            primary_name: string
            secondary_name: string
            implements Named {
                name as primary_name
                name as secondary_name
            }
        }
        "#,
        "E0130",
    );
}

// ── Group AI: explicit requires satisfaction (E0125) ─────────────────────────

#[test]
fn missing_required_parent_interface_is_error() {
    // E0125: Person requires Named + Aged, but Employee only implements Person.
    assert_compile_error_code(
        r#"
        interface Named { name: string }
        interface Aged { age: int }
        interface Person requires Named, Aged {
            occupation: string
        }
        class Bad {
            name: string
            age: int
            occupation: string
            implements Person {}
        }
        "#,
        "E0125",
    );
}

#[test]
fn missing_one_of_two_required_parents_is_error() {
    assert_compile_error_code(
        r#"
        interface Named { name: string }
        interface Aged { age: int }
        interface Person requires Named, Aged {
            occupation: string
        }
        class Partial {
            name: string
            age: int
            occupation: string
            implements Named {}
            implements Person {}
        }
        "#,
        "E0125",
    );
}

#[test]
fn all_required_parents_satisfied_is_ok() {
    assert_zero_compile_errors(
        r#"
        interface Named { name: string }
        interface Aged { age: int }
        interface Person requires Named, Aged {
            occupation: string
        }
        class Employee {
            name: string
            age: int
            occupation: string
            salary: float
            implements Named {}
            implements Aged {}
            implements Person {}
        }
        "#,
    );
}

#[test]
fn required_parent_lookup_uses_declaring_interface_namespace() {
    let files = &[
        (
            "main.baml",
            r#"
                interface Parent {}

                class Robot {
                    function label(self) -> string { return "robot" }
                    implements root.contracts.Child {}
                    implements Parent {}
                }
                "#,
        ),
        (
            "ns_contracts/interfaces.baml",
            r#"
                interface Parent {
                    function label(self) -> string
                }
                interface Child requires Parent {}
                "#,
        ),
    ];
    assert_compile_error_contains_multi(files, "E0125");
}

#[tokio::test]
async fn requires_chain_exposes_parent_fields() {
    // Person requires Named — accessing `name` through a Person-typed
    // variable should work because Person inherits Named's fields.
    let output = baml_test!(
        r#"
        interface Named { name: string }
        interface Person requires Named {
            occupation: string
        }
        class Employee {
            name: string
            occupation: string
            implements Named {}
            implements Person {}
        }
        function main() -> string {
            let p: Person = Employee { name: "Alice", occupation: "PM" }
            return p.name
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Alice".into())
    );
}

#[tokio::test]
async fn requires_chain_parent_field_in_default_method() {
    // Person requires Named — Person's default method should be able
    // to access `self.name` since Named provides it.
    let output = baml_test!(
        r#"
        interface Named { name: string }
        interface Person requires Named {
            occupation: string
            function introduce(self) -> string {
                return self.name + " the " + self.occupation
            }
        }
        class Employee {
            name: string
            occupation: string
            implements Named {}
            implements Person {}
        }
        function main() -> string {
            let p: Person = Employee { name: "Alice", occupation: "PM" }
            return p.introduce()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Alice the PM".into())
    );
}

#[test]
fn user_scenario_requires_field_check() {
    let errors = collect_compile_errors(
        r#"
        interface A {
            foo: string
        }
        interface B requires A {}
        class Blah {
            foo: string
            implements B {}
            implements A {}
        }
        "#,
    );
    assert!(
        errors.is_empty(),
        "expected zero errors, got:\n  {}",
        errors.join("\n  ")
    );
}

#[test]
fn interface_requires_conflicting_field_types_is_error() {
    assert_compile_error_code(
        r#"
        interface X {
            id: string
        }
        interface Y {
            id: int
        }
        interface Z requires X, Y {}
        "#,
        "E0122",
    );
}

// ── Group AJ: out-of-body implements (`implements I for T`) ─────────────────

#[test]
fn out_of_body_implements_for_class_compiles() {
    assert_zero_compile_errors(
        r#"
        interface ToJson {
            function to_json(self) -> string
        }
        class Dog { breed: string }
        implements ToJson for Dog {
            function to_json(self) -> string { return self.breed }
        }
        "#,
    );
}

#[test]
fn out_of_body_implements_for_qualified_generic_class_uses_class_methods() {
    let files = &[
        (
            "main.baml",
            r#"
                interface Printable {
                    function label(self) -> string
                }

                implements Printable for root.models.Box<int> {}
                "#,
        ),
        (
            "ns_models/box.baml",
            r#"
                class Box<T> {
                    value: T

                    function label(self) -> string {
                        return "box"
                    }
                }
                "#,
        ),
    ];
    assert_no_compile_errors_multi(files);
}

#[test]
fn out_of_body_implements_field_bearing_interface_is_error() {
    assert_compile_error_code(
        r#"
        interface Named {
            name: string
            function greet(self) -> string
        }
        class Robot { model: string }
        implements Named for Robot {
            name: string
            function greet(self) -> string { return "I am " + self.name }
        }
        "#,
        "E0126",
    );
}

#[test]
fn out_of_body_implements_field_bearing_interface_is_error_even_without_redeclared_fields() {
    assert_compile_error_code(
        r#"
        interface Named {
            name: string
            function greet(self) -> string
        }
        class Robot { model: string }
        implements Named for Robot {
            function greet(self) -> string { return "I am a robot" }
        }
        "#,
        "E0126",
    );
}

#[test]
fn out_of_body_implements_inherited_field_bearing_interface_is_error() {
    assert_compile_error_code(
        r#"
        interface Named {
            name: string
        }
        interface Child requires Named {}
        class Robot { model: string }

        implements Child for Robot {}
        "#,
        "E0126",
    );
}

#[tokio::test]
async fn out_of_body_implements_is_visible_to_reflection_registry() {
    let output = baml_test!(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {}
        implements Animal for Dog {
            function speak(self) -> string { return "woof" }
        }
        function main() -> bool {
            let impls = reflect.type_of<Animal>().implementors()
            return reflect.type_of<Dog>().implements(reflect.type_of<Animal>())
                && impls.length() == 1
                && impls[0] == reflect.type_of<Dog>()
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

#[test]
fn out_of_body_and_in_body_for_same_interface_is_error() {
    assert_compile_error_code(
        r#"
        interface ToJson {
            function to_json(self) -> string
        }
        class Dog {
            implements ToJson {
                function to_json(self) -> string { return "woof" }
            }
        }
        implements ToJson for Dog {
            function to_json(self) -> string { return "bark" }
        }
        "#,
        "E0114",
    );
}

#[test]
fn out_of_body_implements_for_unknown_target_is_error() {
    let errors = collect_compile_errors(
        r#"
        interface ToJson {
            function to_json(self) -> string
        }
        implements ToJson for Nonexistent {
            function to_json(self) -> string { return "?" }
        }
        "#,
    );
    assert!(
        errors.iter().any(|e| e.contains("Nonexistent")),
        "expected error about unknown target type `Nonexistent`; got:\n  {}",
        errors.join("\n  ")
    );
}

#[test]
fn out_of_body_implement_singular_keyword_compiles() {
    assert_zero_compile_errors(
        r#"
        interface ToJson {
            function to_json(self) -> string
        }
        class Cat { color: string }
        implement ToJson for Cat {
            function to_json(self) -> string { return self.color }
        }
        "#,
    );
}

#[test]
fn out_of_body_implements_for_primitive_method_only_compiles() {
    assert_zero_compile_errors(
        r#"
        interface Debuggable {
            function debug(self) -> string
        }
        implements Debuggable for int {
            function debug(self) -> string { return "int" }
        }
        "#,
    );
}

#[test]
fn out_of_body_implements_for_primitive_satisfies_interface_type() {
    assert_zero_compile_errors(
        r#"
        interface Debuggable {
            function debug(self) -> string
        }
        implements Debuggable for int {
            function debug(self) -> string { return "int" }
        }
        function use_debuggable() -> string {
            let value: int = 1
            let as_debuggable: Debuggable = value
            return as_debuggable.debug()
        }
        "#,
    );
}

#[test]
fn out_of_body_implements_for_generic_function_rejects_non_generic_function_value() {
    assert_compile_error_contains(
        r#"
        interface GenericCallable {}

        implements GenericCallable for <T>(x: int) -> int {}

        function concrete(x: int) -> int {
            return x
        }

        function main() -> void {
            let f: (x: int) -> int = concrete
            let marker: GenericCallable = f
        }
        "#,
        "type mismatch",
    );
}

#[test]
fn out_of_body_implements_for_generic_function_rejects_different_bound() {
    assert_compile_error_contains(
        r#"
        interface GenericCallable {}
        interface Readable {}
        interface Writable {}

        implements GenericCallable for <T extends Readable>(x: int) -> int {}

        function writable<U extends Writable>(x: int) -> int {
            return x
        }

        function main() -> void {
            let marker: GenericCallable = writable
        }
        "#,
        "type mismatch",
    );
}

#[test]
fn out_of_body_implements_for_primitive_as_projection_compiles() {
    assert_zero_compile_errors(
        r#"
        interface Debuggable {
            function debug(self) -> string
        }
        implements Debuggable for int {
            function debug(self) -> string { return "int" }
        }
        function use_debuggable() -> string {
            let myInteger: int = 1
            return myInteger.as<Debuggable>.debug()
        }
        "#,
    );
}

#[tokio::test]
async fn out_of_body_implements_for_primitive_as_projection_runtime() {
    let output = baml_test!(
        r#"
        interface Debuggable {
            function debug(self) -> string
        }
        implements Debuggable for int {
            function debug(self) -> string { return "int" }
        }
        function main() -> string {
            let myInteger: int = 1
            let asDebuggable: Debuggable = myInteger
            return myInteger.as<Debuggable>.debug() + ":" + asDebuggable.debug()
        }
        "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("int:int".into())
    );
}

#[tokio::test]
async fn default_call_from_out_of_body_override_runtime() {
    let output = baml_test!(
        r#"
        interface Logger {
            function log(self, msg: string) -> string {
                return "[L] " + msg
            }
        }
        class PrefixLogger {
            prefix: string
        }
        implements Logger for PrefixLogger {
            function log(self, msg: string) -> string {
                return self.prefix + default.log(msg)
            }
        }
        function main() -> string {
            let logger: Logger = PrefixLogger { prefix: "P:" }
            return logger.log("hi")
        }
        "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("P:[L] hi".into())
    );
}

#[tokio::test]
async fn default_call_from_generic_out_of_body_override_runtime() {
    let output = baml_test!(
        r#"
        interface Logger {
            function log(self, msg: string) -> string {
                return "[L] " + msg
            }
        }
        class Box<T> {
            prefix: string
            value: T
        }
        implements<T> Logger for Box<T> {
            function log(self, msg: string) -> string {
                return self.prefix + default.log(msg)
            }
        }
        function main() -> string {
            let logger: Logger = Box<int> { prefix: "P:", value: 1 }
            return logger.log("hi")
        }
        "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("P:[L] hi".into())
    );
}

#[tokio::test]
async fn generic_requires_parent_args_dispatch_on_class_implementor() {
    let output = baml_test!(
        r#"
        interface Parent<T> {
            function describe(self) -> string
        }
        interface Child<T> requires Parent<T> {}
        class Box {
            implements Parent<int> {
                function describe(self) -> string { return "parent-int" }
            }
            implements Child<int> {}
        }
        function main() -> string {
            let child: Child<int> = Box {}
            return child.describe()
        }
        "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("parent-int".into())
    );
}

#[test]
fn generic_required_parent_args_must_match() {
    assert_compile_error_code(
        r#"
        interface Parent<T> {}
        interface Child<T> requires Parent<T> {}

        class Box {
            implements Parent<string> {}
            implements Child<int> {}
        }
        "#,
        "E0125",
    );
}

#[test]
fn generic_required_parent_args_accept_alias_equivalent_type() {
    assert_zero_compile_errors(
        r#"
        type Texty = string | int

        interface Parent<T> {}
        interface Child requires Parent<int | string> {}

        class Box {
            implements Parent<Texty> {}
            implements Child {}
        }
        "#,
    );
}

#[test]
fn out_of_body_generic_required_parent_args_must_match() {
    assert_compile_error_code(
        r#"
        interface Parent<T> {}
        interface Child<T> requires Parent<T> {}

        implements Parent<string> for int {}
        implements Child<int> for int {}
        "#,
        "E0125",
    );
}

#[test]
fn inherited_generic_interface_field_construction_uses_parent_args() {
    assert_compile_error_contains(
        r#"
        interface Parent<T> {
            value: T
        }
        interface Child<T> requires Parent<T> {}
        class Box {
            stored: int
            implements Child<int> {}
            implements Parent<int> {
                value as stored
            }
        }
        function main() -> void {
            let b = Box { value: "wrong" }
        }
        "#,
        "type mismatch: expected int",
    );
}

#[tokio::test]
async fn generic_requires_parent_field_args_runtime() {
    let output = baml_test!(
        r#"
        interface Parent<T> {
            value: T
        }
        interface Child<T> requires Parent<T> {}
        class Box {
            value: int
            implements Parent<int> {}
            implements Child<int> {}
        }
        function main() -> int {
            let child: Child<int> = Box { value: 42 }
            return child.value
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(42));
}

#[tokio::test]
async fn generic_requires_parent_field_alias_runtime() {
    let output = baml_test!(
        r#"
        interface Parent<T> {
            value: T
        }
        interface Child<T> requires Parent<T> {}
        class Box {
            stored: int
            implements Parent<int> {
                value as stored
            }
            implements Child<int> {}
        }
        function main() -> int {
            let child: Child<int> = Box { stored: 42 }
            return child.value
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(42));
}

#[tokio::test]
async fn generic_requires_parent_args_dispatch_on_type_implementor() {
    let output = baml_test!(
        r#"
        interface Parent<T> {
            function describe(self) -> string
        }
        interface Child<T> requires Parent<T> {}
        implements Parent<int> for int {
            function describe(self) -> string { return "parent-int" }
        }
        implements Child<int> for int {}
        function main() -> string {
            let child: Child<int> = 1
            return child.describe()
        }
        "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("parent-int".into())
    );
}

#[tokio::test]
async fn requires_closure_preserves_multiple_parent_instantiations_runtime() {
    let output = baml_test!(
        r#"
        interface Parent<T> {
            function describe(self) -> string
        }
        interface NeedsInt requires Parent<int> {}
        interface NeedsString requires Parent<string> {}
        interface Both requires NeedsInt, NeedsString {}
        class Box {
            implements Parent<int> {
                function describe(self) -> string { return "int" }
            }
            implements Parent<string> {
                function describe(self) -> string { return "string" }
            }
            implements NeedsInt {}
            implements NeedsString {}
            implements Both {}
        }
        function main() -> bool {
            let both: Both = Box {}
            return both.as<Parent<int>>.describe() == "int"
                && both.as<Parent<string>>.describe() == "string"
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

#[test]
fn generic_interface_default_method_reference_compiles() {
    assert_no_compile_errors(
        r#"
        interface Label<T> {
            function label(self) -> string {
                return "ok"
            }
        }
        class Box {
            implements Label<int> {}
        }
        function main() -> void {
            let label = Label.label
        }
        "#,
    );
}

#[test]
fn out_of_body_implements_for_primitive_field_bearing_interface_is_error() {
    assert_compile_error_code(
        r#"
        interface Named {
            name: string
            function display(self) -> string
        }
        implements Named for int {
            function display(self) -> string { return "int" }
        }
        "#,
        "E0126",
    );
}

#[test]
fn out_of_body_implements_for_primitive_cannot_add_fields() {
    assert_compile_error_contains(
        r#"
        interface Named {
            name: string
        }
        implements Named for int {
            name: string
        }
        "#,
        "field",
    );
}

#[test]
fn out_of_body_method_callable_on_instance() {
    assert_zero_compile_errors(
        r#"
        interface Describable {
            function describe(self) -> string
        }
        class Car { make: string }
        implements Describable for Car {
            function describe(self) -> string { return self.make }
        }
        function run_describe() -> string {
            let c = Car { make: "Toyota" }
            return c.as<Describable>.describe()
        }
        "#,
    );
}

#[test]
fn out_of_body_dispatch_through_interface_typed_var() {
    assert_zero_compile_errors(
        r#"
        interface Speakable {
            function speak(self) -> string
        }
        class Parrot { phrase: string }
        implements Speakable for Parrot {
            function speak(self) -> string { return self.phrase }
        }
        function run_speak(s: Speakable) -> string {
            return s.speak()
        }
        "#,
    );
}

// ── Group AK: interface-to-interface subtyping (Phase 6) ─────────────────────

#[test]
fn cross_interface_assignment_is_compile_error() {
    assert_compile_error_contains(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        interface Swimmer {
            function swim(self) -> string
        }
        class Duck {
            implements Animal {
                function speak(self) -> string { return "Quack!" }
            }
            implements Swimmer {
                function swim(self) -> string { return "splash" }
            }
        }
        function bad() -> string {
            let a: Animal = Duck {}
            let s: Swimmer = a
            return s.swim()
        }
        "#,
        "Animal",
    );
}

#[test]
fn same_interface_assignment_is_ok() {
    assert_zero_compile_errors(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        function ok() -> string {
            let a: Animal = Dog {}
            let b: Animal = a
            return b.speak()
        }
        "#,
    );
}

#[test]
fn requires_chain_interface_subtype_is_ok() {
    assert_zero_compile_errors(
        r#"
        interface Named {
            name: string
        }
        interface Person requires Named {
            occupation: string
        }
        class Employee {
            name: string
            occupation: string
            salary: float
            implements Named {}
            implements Person {}
        }
        function ok(p: Person) -> string {
            let n: Named = p
            return n.name
        }
        "#,
    );
}

// ── Group: Blanket implementations — Phase 1 (parsing) ────────────────────

#[test]
fn form1_syntax_parses_without_errors() {
    assert_no_interface_errors(
        r#"
        interface Printable {
            function display(self) -> string
        }
        class Box<T> {
            value: T
        }
        implements<T> Printable for Box<T> {
            function display(self) -> string { return "a box" }
        }
    "#,
    );
}

#[test]
fn form1_bounded_syntax_parses_without_errors() {
    assert_no_interface_errors(
        r#"
        interface Named {
            name: string
        }
        interface Printable {
            function display(self) -> string
        }
        class Wrapper<T> {
            inner: T
        }
        implements<T extends Named> Printable for Wrapper<T> {
            function display(self) -> string { return "a wrapper" }
        }
    "#,
    );
}

#[test]
fn form2_syntax_parses_without_errors() {
    assert_no_interface_errors(
        r#"
        interface Named {
            name: string
        }
        interface Printable {
            function display(self) -> string
        }
        implements<T extends Named> Printable for T {
            function display(self) -> string { return "named thing" }
        }
    "#,
    );
}

#[test]
fn existing_concrete_implements_for_still_works() {
    assert_no_interface_errors(
        r#"
        interface Debuggable {
            function debug(self) -> string
        }
        implements Debuggable for int {
            function debug(self) -> string { return "int" }
        }
    "#,
    );
}

// ── Group: Blanket implementations — Phase 2 (Form 1 runtime) ─────────────

#[tokio::test]
async fn form1_dispatches_through_interface_typed_var() {
    let output = baml_test!(
        r#"
        interface Printable {
            function display(self) -> string
        }
        class Box<T> {
            value: T
        }
        implements<T> Printable for Box<T> {
            function display(self) -> string { return "a box" }
        }
        function main() -> string {
            let b: Printable = Box<int> { value: 42 }
            return b.display()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("a box".into())
    );
}

#[tokio::test]
async fn form1_dispatches_for_different_instantiations() {
    let output = baml_test!(
        r#"
        interface Printable {
            function display(self) -> string
        }
        class Box<T> {
            value: T
        }
        implements<T> Printable for Box<T> {
            function display(self) -> string { return "a box" }
        }
        function main() -> string {
            let a: Printable = Box<int> { value: 42 }
            let b: Printable = Box<string> { value: "hi" }
            return a.display() + " " + b.display()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("a box a box".into())
    );
}

#[tokio::test]
async fn form1_self_accesses_receiver_fields() {
    let output = baml_test!(
        r#"
        interface Describable {
            function describe(self) -> string
        }
        class Pair<T> {
            first: T
            second: T
        }
        implements<T> Describable for Pair<T> {
            function describe(self) -> string { return "a pair" }
        }
        function main() -> string {
            let p: Describable = Pair<int> { first: 1, second: 2 }
            return p.describe()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("a pair".into())
    );
}

#[tokio::test]
async fn form1_with_generic_interface_args() {
    let output = baml_test!(
        r#"
        interface Container<T> {
            function get(self) -> T
        }
        class Wrapper<T> {
            value: T
        }
        implements<T> Container<T> for Wrapper<T> {
            function get(self) -> T { return self.value }
        }
        function main() -> int {
            let w: Container<int> = Wrapper<int> { value: 42 }
            return w.get()
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(42));
}

#[tokio::test]
async fn generic_rule_for_list_receiver_dispatches() {
    let output = baml_test!(
        r#"
        interface Label {
            function label(self) -> string
        }
        implements<T> Label for T[] {
            function label(self) -> string { return "list" }
        }
        function main() -> string {
            let xs: int[] = [1, 2, 3]
            let labelled: Label = xs
            return labelled.label()
        }
        "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("list".into())
    );
}

#[tokio::test]
async fn generic_rule_for_map_receiver_dispatches() {
    let output = baml_test!(
        r#"
        interface Label {
            function label(self) -> string
        }
        implements<T> Label for map<string, T> {
            function label(self) -> string { return "map" }
        }
        function main() -> string {
            let values: map<string, int> = { "a": 1 }
            let labelled: Label = values
            return labelled.label()
        }
        "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("map".into())
    );
}

#[tokio::test]
async fn generic_rule_for_optional_receiver_dispatches() {
    let output = baml_test!(
        r#"
        interface Label {
            function label(self) -> string
        }
        implements<T> Label for T? {
            function label(self) -> string { return "optional" }
        }
        function main() -> string {
            let value: int? = 1
            let labelled: Label = value
            return labelled.label()
        }
        "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("optional".into())
    );
}

#[test]
fn generic_rule_for_list_receiver_overlaps_concrete_list() {
    assert_compile_error_code(
        r#"
        interface Label {
            function label(self) -> string
        }
        implements Label for int[] {
            function label(self) -> string { return "ints" }
        }
        implements<T> Label for T[] {
            function label(self) -> string { return "list" }
        }
        "#,
        "E0132",
    );
}

#[test]
fn form1_coexists_with_concrete_impl_for_different_class() {
    assert_no_interface_errors(
        r#"
        interface Printable {
            function display(self) -> string
        }
        class Box<T> {
            value: T
        }
        class Leaf {
            label: string
        }
        implements<T> Printable for Box<T> {
            function display(self) -> string { return "box" }
        }
        implements Printable for Leaf {
            function display(self) -> string { return "leaf" }
        }
    "#,
    );
}

#[test]
fn form1_blanket_has_no_compile_errors_at_all() {
    let errors = collect_compile_errors(
        r#"
        interface Printable {
            function display(self) -> string
        }
        class Box<T> {
            value: T
        }
        implements<T> Printable for Box<T> {
            function display(self) -> string { return "a box" }
        }
    "#,
    );
    assert!(
        errors.is_empty(),
        "expected zero errors, got:\n  {}",
        errors.join("\n  ")
    );
}

#[test]
fn unified_rule_rejects_mismatched_generic_interface_arg() {
    assert_compile_error_contains(
        r#"
        interface Container<T> {
            function get(self) -> T
        }
        class Wrapper<T> {
            value: T
        }
        implements<T> Container<T> for Wrapper<T> {
            function get(self) -> T { return self.value }
        }
        function take(c: Container<string>) -> string {
            return c.get()
        }
        function bad() -> string {
            return take(Wrapper<int> { value: 42 })
        }
        "#,
        "Wrapper<int>",
    );
}

#[tokio::test]
async fn unified_rule_nested_interface_args_dispatch() {
    let output = baml_test!(
        r#"
        interface Container<T> {
            function get(self) -> T
        }
        class Wrapper<T> {
            values: T[]
        }
        implements<T> Container<T[]> for Wrapper<T> {
            function get(self) -> T[] { return self.values }
        }
        function main() -> int {
            let w: Container<int[]> = Wrapper<int> { values: [1, 2, 3] }
            return w.get().length()
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(3));
}

#[tokio::test]
async fn unified_rule_repeated_type_vars_match_runtime() {
    let output = baml_test!(
        r#"
        interface Same {
            function tag(self) -> string
        }
        class Pair<L, R> {
            left: L
            right: R
        }
        implements<T> Same for Pair<T, T> {
            function tag(self) -> string { return "same" }
        }
        function main() -> string {
            let p: Same = Pair<int, int> { left: 7, right: 8 }
            return p.tag()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("same".into())
    );
}

#[test]
fn unified_rule_repeated_type_vars_reject_conflicting_args() {
    assert_compile_error_contains(
        r#"
        interface Same {
            function tag(self) -> string
        }
        class Pair<L, R> {
            left: L
            right: R
        }
        implements<T> Same for Pair<T, T> {
            function tag(self) -> string { return "same" }
        }
        function take(p: Same) -> string {
            return p.tag()
        }
        function bad() -> string {
            return take(Pair<int, string> { left: 7, right: "nope" })
        }
        "#,
        "Pair<int, string>",
    );
}

#[tokio::test]
async fn unified_rule_default_method_inherited_through_generic_rule() {
    let output = baml_test!(
        r#"
        interface Printable {
            function display(self) -> string { return "default" }
        }
        class Box<T> {
            value: T
        }
        implements<T> Printable for Box<T> {}
        function main() -> string {
            let b: Printable = Box<int> { value: 42 }
            return b.display()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("default".into())
    );
}

#[tokio::test]
async fn unified_rule_reflection_sees_generic_class_implementor_once() {
    let output = baml_test!(
        r#"
        interface Printable {
            function display(self) -> string
        }
        class Box<T> {
            value: T
        }
        implements<T> Printable for Box<T> {
            function display(self) -> string { return "box" }
        }
        function main() -> bool {
            let impls = reflect.type_of<Printable>().implementors()
            return reflect.type_of<Box<int>>().implements(reflect.type_of<Printable>())
                && impls.length() == 1
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

#[tokio::test]
async fn form2_dispatches_through_interface_typed_var() {
    let output = baml_test!(
        r#"
        interface Named {
            name: string
        }
        interface Printable {
            function display(self) -> string
        }
        class Person {
            name: string
            implements Named {}
        }
        implements<T extends Named> Printable for T {
            function display(self) -> string { return "named:" + self.name }
        }
        function main() -> string {
            let p: Printable = Person { name: "Ada" }
            return p.display()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("named:Ada".into())
    );
}

#[tokio::test]
async fn form2_self_accesses_bound_members() {
    let output = baml_test!(
        r#"
        interface Named {
            name: string
        }
        interface Labeled {
            function label(self) -> string
        }
        class Project {
            name: string
            implements Named {}
        }
        implements<T extends Named> Labeled for T {
            function label(self) -> string { return self.name }
        }
        function main() -> string {
            let item: Labeled = Project { name: "Launch" }
            return item.label()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Launch".into())
    );
}

#[tokio::test]
async fn form2_applies_to_multiple_satisfying_classes() {
    let output = baml_test!(
        r#"
        interface Named {
            name: string
        }
        interface Printable {
            function display(self) -> string
        }
        class Person {
            name: string
            implements Named {}
        }
        class Team {
            name: string
            implements Named {}
        }
        implements<T extends Named> Printable for T {
            function display(self) -> string { return self.name }
        }
        function main() -> string {
            let person: Printable = Person { name: "Ada" }
            let team: Printable = Team { name: "Core" }
            return person.display() + "/" + team.display()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Ada/Core".into())
    );
}

#[test]
fn form2_does_not_apply_when_bound_not_satisfied() {
    assert_compile_error_contains(
        r#"
        interface Named {
            name: string
        }
        interface Printable {
            function display(self) -> string
        }
        class Rock {
            label: string
        }
        implements<T extends Named> Printable for T {
            function display(self) -> string { return self.name }
        }
        function bad() -> string {
            let item: Printable = Rock { label: "igneous" }
            return item.display()
        }
        "#,
        "Rock",
    );
}

#[tokio::test]
async fn form2_reflect_implements_returns_true() {
    let output = baml_test!(
        r#"
        interface Named {
            name: string
        }
        interface Printable {
            function display(self) -> string
        }
        class Person {
            name: string
            implements Named {}
        }
        implements<T extends Named> Printable for T {
            function display(self) -> string { return self.name }
        }
        function main() -> bool {
            return reflect.type_of<Person>().implements(reflect.type_of<Printable>())
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

#[tokio::test]
async fn form2_reflect_implementors_includes_satisfying_classes() {
    let output = baml_test!(
        r#"
        interface Named {
            name: string
        }
        interface Printable {
            function display(self) -> string
        }
        class Person {
            name: string
            implements Named {}
        }
        class Team {
            name: string
            implements Named {}
        }
        implements<T extends Named> Printable for T {
            function display(self) -> string { return self.name }
        }
        function main() -> bool {
            let printable = reflect.type_of<Printable>()
            return printable.implemented_by(reflect.type_of<Person>())
                && printable.implemented_by(reflect.type_of<Team>())
                && printable.implementors().length() == 2
        }
    "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

#[tokio::test]
async fn unified_rule_implementor_satisfies_generic_interface_bound() {
    let output = baml_test!(
        r#"
        interface Printable {
            function display(self) -> string
        }
        class Box<T> {
            value: T
        }
        implements<T> Printable for Box<T> {
            function display(self) -> string { return "box" }
        }
        function f<T extends Printable>(x: T) -> string {
            return x.display()
        }
        function main() -> string {
            return f<Box<int>>(Box<int> { value: 42 })
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("box".into())
    );
}

#[tokio::test]
async fn unified_rule_implementor_satisfies_inferred_generic_interface_bound() {
    let output = baml_test!(
        r#"
        interface Printable {
            function display(self) -> string
        }
        class Box<T> {
            value: T
        }
        implements<T> Printable for Box<T> {
            function display(self) -> string { return "box" }
        }
        function f<T extends Printable>(x: T) -> string {
            return x.display()
        }
        function main() -> string {
            return f(Box<int> { value: 42 })
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("box".into())
    );
}

#[tokio::test]
async fn unified_rule_implementor_satisfies_inferred_generic_bound_from_constructor_args() {
    let output = baml_test!(
        r#"
        interface Printable {
            function display(self) -> string
        }
        class Box<T> {
            value: T
        }
        implements<T> Printable for Box<T> {
            function display(self) -> string { return "box" }
        }
        function f<T extends Printable>(x: T) -> string {
            return x.display()
        }
        function main() -> string {
            return f(Box { value: 42 })
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("box".into())
    );
}

#[tokio::test]
async fn bounded_type_var_rule_satisfies_generic_interface_bound() {
    let output = baml_test!(
        r#"
        interface Named {
            name: string
        }
        interface Printable {
            function display(self) -> string
        }
        class Person {
            name: string
            implements Named {}
        }
        implements<T extends Named> Printable for T {
            function display(self) -> string { return self.name }
        }
        function f<T extends Printable>(x: T) -> string {
            return x.display()
        }
        function main() -> string {
            return f<Person>(Person { name: "Ada" })
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Ada".into())
    );
}

#[tokio::test]
async fn bounded_type_var_rule_satisfies_inferred_generic_interface_bound() {
    let output = baml_test!(
        r#"
        interface Named {
            name: string
        }
        interface Printable {
            function display(self) -> string
        }
        class Person {
            name: string
            implements Named {}
        }
        implements<T extends Named> Printable for T {
            function display(self) -> string { return self.name }
        }
        function f<T extends Printable>(x: T) -> string {
            return x.display()
        }
        function main() -> string {
            return f(Person { name: "Ada" })
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Ada".into())
    );
}

#[test]
fn interface_method_reference_accepts_bounded_generic_function_annotation() {
    assert_no_compile_errors(
        r#"
        interface MyInterface {
            function myMethod(self) -> int
        }
        class MyClass {
            implements MyInterface {
                function myMethod(self) -> int {
                    return 1
                }
            }
        }
        function main() -> void {
            let method : <T extends MyInterface>(T) -> int = MyInterface.myMethod
        }
    "#,
    );
}

#[test]
fn inferred_interface_method_reference_enforces_receiver_bound() {
    assert_no_compile_errors(
        r#"
        interface MyInterface {
            function myMethod(self) -> int
        }
        class MyClass {
            implements MyInterface {
                function myMethod(self) -> int {
                    return 1
                }
            }
        }
        function main() -> int {
            let method = MyInterface.myMethod
            return method(MyClass {})
        }
    "#,
    );
}

#[test]
fn inferred_interface_method_reference_rejects_receiver_outside_bound() {
    assert_compile_error_contains(
        r#"
        interface MyInterface {
            function myMethod(self) -> int
        }
        class Other {}
        function main() -> int {
            let method = MyInterface.myMethod
            return method(Other {})
        }
    "#,
        "MyInterface",
    );
}

#[tokio::test]
async fn form1_bounded_generic_receiver_dispatches_when_bound_satisfied() {
    let output = baml_test!(
        r#"
        interface Named {
            name: string
        }
        interface Printable {
            function display(self) -> string
        }
        class Person {
            name: string
            implements Named {}
        }
        class Wrapper<T> {
            inner: T
        }
        implements<T extends Named> Printable for Wrapper<T> {
            function display(self) -> string { return self.inner.name }
        }
        function main() -> string {
            let item: Printable = Wrapper<Person> { inner: Person { name: "Ada" } }
            return item.display()
        }
    "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Ada".into())
    );
}

#[test]
fn form1_bounded_generic_receiver_rejects_when_bound_not_satisfied() {
    let source = r#"
    interface Named {
        name: string
    }
    interface Printable {
        function display(self) -> string
    }
    class Rock {
        label: string
    }
    class Wrapper<T> {
        inner: T
    }
    implements<T extends Named> Printable for Wrapper<T> {
        function display(self) -> string { return self.inner.name }
    }
    function bad() -> string {
        let item: Printable = Wrapper<Rock> { inner: Rock { label: "igneous" } }
        return item.display()
    }
    "#;
    assert_compile_error_contains(source, "Wrapper");
    assert_compile_error_contains(source, "Rock");
}

#[test]
fn overlapping_concrete_and_generic_rules_are_e0132() {
    assert_compile_error_code(
        r#"
        interface Printable {
            function display(self) -> string
        }
        class Box<T> {
            value: T
        }
        implements<T> Printable for Box<T> {
            function display(self) -> string { return "box" }
        }
        implements Printable for Box<int> {
            function display(self) -> string { return "int box" }
        }
        "#,
        "E0132",
    );
}

#[test]
fn overlapping_generic_rules_are_e0132() {
    assert_compile_error_code(
        r#"
        interface Printable {
            function display(self) -> string
        }
        class Box<T> {
            value: T
        }
        implements<T> Printable for Box<T> {
            function display(self) -> string { return "box" }
        }
        implements<U> Printable for Box<U> {
            function display(self) -> string { return "other box" }
        }
        "#,
        "E0132",
    );
}

#[test]
fn overlapping_in_body_and_out_of_body_generic_rules_are_e0132() {
    assert_compile_error_code(
        r#"
        interface Printable {
            function display(self) -> string
        }
        class Box<T> {
            value: T
            implements Printable {
                function display(self) -> string { return "in body" }
            }
        }
        implements<T> Printable for Box<T> {
            function display(self) -> string { return "out of body" }
        }
        "#,
        "E0132",
    );
}

#[test]
fn form2_overlap_with_form2_is_e0132() {
    assert_compile_error_code(
        r#"
        interface Named {
            name: string
        }
        interface Printable {
            function display(self) -> string
        }
        implements<T extends Named> Printable for T {
            function display(self) -> string { return "first" }
        }
        implements<U extends Named> Printable for U {
            function display(self) -> string { return "second" }
        }
        "#,
        "E0132",
    );
}

#[test]
fn overlapping_bounded_generic_receiver_rules_are_e0132() {
    assert_compile_error_code(
        r#"
        interface Named {
            name: string
        }
        interface Tagged {
            tag: string
        }
        interface Printable {
            function display(self) -> string
        }
        class Box<T> {
            value: T
        }
        implements<T extends Named> Printable for Box<T> {
            function display(self) -> string { return "named box" }
        }
        implements<U extends Tagged> Printable for Box<U> {
            function display(self) -> string { return "tagged box" }
        }
        "#,
        "E0132",
    );
}

#[test]
fn overlapping_bounded_and_unbounded_generic_receiver_rules_are_e0132() {
    assert_compile_error_code(
        r#"
        interface Named {
            name: string
        }
        interface Printable {
            function display(self) -> string
        }
        class Box<T> {
            value: T
        }
        implements<T> Printable for Box<T> {
            function display(self) -> string { return "box" }
        }
        implements<U extends Named> Printable for Box<U> {
            function display(self) -> string { return "named box" }
        }
        "#,
        "E0132",
    );
}

#[test]
fn non_overlapping_generic_receiver_rules_for_different_classes_are_ok() {
    assert_no_interface_errors(
        r#"
        interface Printable {
            function display(self) -> string
        }
        class Box<T> {
            value: T
        }
        class Envelope<T> {
            value: T
        }
        implements<T> Printable for Box<T> {
            function display(self) -> string { return "box" }
        }
        implements<T> Printable for Envelope<T> {
            function display(self) -> string { return "envelope" }
        }
        "#,
    );
}

#[test]
fn bounded_type_var_rule_conservatively_overlaps_concrete_rule() {
    assert_compile_error_code(
        r#"
        interface Named {
            name: string
        }
        interface Printable {
            function display(self) -> string
        }
        class User {
            name: string
            implements Named {}
        }
        implements Printable for User {
            function display(self) -> string { return "user" }
        }
        implements<T extends Named> Printable for T {
            function display(self) -> string { return "named" }
        }
        "#,
        "E0132",
    );
}

#[test]
fn unified_rule_namespaced_classes_with_same_short_name_do_not_cross_match() {
    let files = &[
        (
            "main.baml",
            r#"
                function take(item: root.a.Printable) -> string {
                    return item.display()
                }
                function bad() -> string {
                    return take(root.b.Wrapper { value: 42 })
                }
                "#,
        ),
        (
            "ns_a/wrapper.baml",
            r#"
                interface Printable {
                    function display(self) -> string
                }
                class Wrapper {
                    value: int
                }
                implements Printable for Wrapper {
                    function display(self) -> string { return "a" }
                }
                "#,
        ),
        (
            "ns_b/wrapper.baml",
            r#"
                class Wrapper {
                    value: int
                }
                "#,
        ),
    ];
    assert_compile_error_contains_multi(files, "b.Wrapper");
}

#[test]
fn unified_rule_namespaced_generic_classes_with_same_short_name_do_not_cross_match() {
    let files = &[
        (
            "main.baml",
            r#"
                function take(item: root.a.Printable<int>) -> string {
                    return item.display()
                }
                function bad(item: root.b.Wrapper<int>) -> string {
                    return take(item)
                }
                "#,
        ),
        (
            "ns_a/wrapper.baml",
            r#"
                interface Printable<T> {
                    function display(self) -> string
                }
                class Wrapper<T> {
                    value: T
                }
                implements<T> Printable<T> for Wrapper<T> {
                    function display(self) -> string { return "a" }
                }
                "#,
        ),
        (
            "ns_b/wrapper.baml",
            r#"
                class Wrapper<T> {
                    value: T
                }
                "#,
        ),
    ];
    assert_compile_error_contains_multi(files, "b.Wrapper<int>");
}

#[test]
fn namespaced_class_can_implement_root_qualified_interface() {
    let files = &[
        (
            "main.baml",
            r#"
                function main() -> string {
                    let d = pets.Dog {}
                    return d.describe()
                }
                "#,
        ),
        (
            "ns_animals/animals.baml",
            r#"
                interface Animal {
                    function sound(self) -> string
                    function describe(self) -> string {
                        return "I say " + self.sound()
                    }
                }
                "#,
        ),
        (
            "ns_pets/pets.baml",
            r#"
                class Dog {
                    implements root.animals.Animal {
                        function sound(self) -> string { return "Woof" }
                    }
                }
                "#,
        ),
    ];

    assert_no_compile_errors_multi(files);
}

#[test]
fn namespaced_class_method_body_resolves_root_qualified_interface_type() {
    let files = &[
        (
            "main.baml",
            r#"
                function main() -> string {
                    let d = pets.Dog {}
                    return d.greet()
                }
                "#,
        ),
        (
            "ns_a/a.baml",
            r#"
                interface Named {
                    function name(self) -> string
                }
                "#,
        ),
        (
            "ns_b/b.baml",
            r#"
                interface Greeter {
                    function greet(self) -> string
                }
                "#,
        ),
        (
            "ns_pets/pets.baml",
            r#"
                class Dog {
                    implements root.a.Named {
                        function name(self) -> string { return "Rex" }
                    }
                    implements root.b.Greeter {
                        function greet(self) -> string {
                            return "Hi, I am " + self.as<root.a.Named>.name()
                        }
                    }
                }
                "#,
        ),
    ];

    assert_no_compile_errors_multi(files);
}

#[test]
fn namespaced_class_cannot_use_unrooted_cross_namespace_qualification() {
    let files = &[
        (
            "ns_a/a.baml",
            r#"
                interface Named {
                    function name(self) -> string
                }
                "#,
        ),
        (
            "ns_pets/pets.baml",
            r#"
                class Dog {
                    implements a.Named {
                        function name(self) -> string { return "Rex" }
                    }
                }
                "#,
        ),
    ];

    assert_compile_error_contains_multi(
        files,
        "class `Dog` cannot implement `a.Named`: no interface with that name is in scope",
    );
}

#[test]
fn qualified_generic_constructor_preserves_concrete_type_in_diagnostics() {
    let files = &[
        (
            "main.baml",
            r#"
                interface Printable<T> {
                    function display(self) -> string
                }
                class Box<T> {
                    value: T
                }
                function take(item: Printable<int>) -> string {
                    return item.display()
                }
                function bad() -> string {
                    return take(root.b.Box<int> { value: 42 })
                }
                "#,
        ),
        (
            "ns_b/box.baml",
            r#"
                class Box<T> {
                    value: T
                }
                "#,
        ),
    ];
    assert_compile_error_contains_multi(files, "b.Box<int>");
}

#[test]
fn qualified_generic_constructor_resolves_child_namespace_type_args() {
    let files = &[
        (
            "main.baml",
            r#"
                function explicit() -> root.b.Box<int> {
                    return root.b.Box<int> { value: 42 }
                }

                function inferred() -> root.b.Box<int> {
                    return root.b.Box { value: 42 }
                }
                "#,
        ),
        (
            "ns_b/box.baml",
            r#"
                class Box<T> {
                    value: T
                }
                "#,
        ),
    ];
    assert_no_compile_errors_multi(files);
}

#[tokio::test]
async fn unified_rule_requires_closure_preserves_substituted_generic_args() {
    let output = baml_test!(
        r#"
        interface Parent<T> {
            function get(self) -> T
        }
        interface Child<T> requires Parent<T> {}
        class Wrapper<T> {
            value: T
        }
        implements<T> Parent<T> for Wrapper<T> {
            function get(self) -> T { return self.value }
        }
        implements<T> Child<T> for Wrapper<T> {}
        function take(child: Child<int>) -> int {
            return child.get()
        }
        function main() -> int {
            return take(Wrapper<int> { value: 42 })
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(42));
}

#[test]
fn bounded_type_var_rule_conservatively_overlaps_generic_class_rule() {
    assert_compile_error_code(
        r#"
        interface Named {
            name: string
        }
        interface Printable {
            function display(self) -> string
        }
        class Box<T> {
            value: T
        }
        implements<T> Printable for Box<T> {
            function display(self) -> string { return "box" }
        }
        implements<U extends Named> Printable for U {
            function display(self) -> string { return "named" }
        }
        "#,
        "E0132",
    );
}

#[test]
fn bounded_type_var_rule_conservatively_overlaps_in_body_generic_class_rule() {
    assert_compile_error_code(
        r#"
        interface Named {
            name: string
        }
        interface Printable {
            function display(self) -> string
        }
        class Box<T> {
            value: T
            implements Printable {
                function display(self) -> string { return "box" }
            }
        }
        implements<U extends Named> Printable for U {
            function display(self) -> string { return "named" }
        }
        "#,
        "E0132",
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// BEP-044 regression suite — derived from a CLI fuzz/stress sweep (45 findings).
//
// Every test below pins behavior that is currently BROKEN. They are expected to
// FAIL on `canary` and to start passing as each defect is fixed. Numbers map to
// _plan/baml_interface_findings.md. Positive cases run the program end-to-end and
// assert the concrete result; "must-reject" cases assert the diagnostic that
// should fire. Finding #30 (a too-weak existing test) is covered by
// `fuzz_bug29_*` below, which asserts the canonical `let d: Dog =>` form narrows.
// ═══════════════════════════════════════════════════════════════════════════

/// Finding #1 [crash]: Interface method reference on required (abstract) method crashes at runtime
#[ignore = "unsupported: taking an interface method as a first-class value \
            (`let f = Interface.method`) and calling it with dynamic dispatch \
            needs a synthesized dispatcher thunk — not implemented"]
#[tokio::test]
async fn fuzz_bug01_method_ref_required_method_crashes() {
    let output = baml_test!(
        r##"interface Animal {
    function speak(self) -> string
}

class Dog {
    implements Animal {
        function speak(self) -> string { return "Woof!" }
    }
}

function main() -> string {
    let speak_fn = Animal.speak
    let d = Dog {}
    return speak_fn(d)
}
"##
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Woof!".into())
    );
}

/// Finding #2 [wrong-result]: Interface method references (default body) always call the default, never dispatch to overrides
#[ignore = "unsupported: an interface method taken as a first-class value \
            (`let f = Interface.method`) doesn't dispatch polymorphically on its \
            receiver — needs a synthesized dispatcher thunk — not implemented"]
#[tokio::test]
async fn fuzz_bug02_method_ref_default_dispatches_to_override() {
    let output = baml_test!(
        r##"interface Greeter {
    function greet(self) -> string {
        return "Hello from default"
    }
}

class FormalGreeter {
    title: string
    implements Greeter {
        function greet(self) -> string { return "Good day, " + self.title }
    }
}

class CasualGreeter {
    name: string
    implements Greeter {
        function greet(self) -> string { return "Hey " + self.name }
    }
}

function main() -> string {
    let greet_fn = Greeter.greet
    let formal = FormalGreeter { title: "Sir" }
    let casual = CasualGreeter { name: "Bob" }
    return greet_fn(formal) + "|" + greet_fn(casual)
}
"##
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Good day, Sir|Hey Bob".into())
    );
}

/// Finding #3 [wrong-result]: Concrete class cannot be assigned to optional interface type (Dog not accepted for Animal?)
#[tokio::test]
async fn fuzz_bug03_implementor_assignable_to_optional_interface_param() {
    let output = baml_test!(
        r##"interface Animal {
    function speak(self) -> string
}

class Dog {
    implements Animal {
        function speak(self) -> string { return "Woof!" }
    }
}

function accepts_optional(a: Animal?) -> string {
    if (a != null) {
        return a.speak()
    } else {
        return "none"
    }
}

function main() -> string {
    let d = Dog {}
    return accepts_optional(d)
}
"##
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Woof!".into())
    );
}

/// Finding #4 [crash]: Calling method directly on parenthesized if-expression with different concrete types in branches crashes
#[tokio::test]
async fn fuzz_bug04_method_call_on_parenthesized_if_expr() {
    let output = baml_test!(
        r##"interface Animal {
    function speak(self) -> string
}

class Dog {
    implements Animal {
        function speak(self) -> string { return "Woof!" }
    }
}

class Cat {
    implements Animal {
        function speak(self) -> string { return "Meow." }
    }
}

function main() -> string {
    return (if (true) { Dog {} } else { Cat {} }).speak()
}
"##
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Woof!".into())
    );
}

/// Finding #5 [wrong-result]: dispatch leaks child override into parent projection when B requires A and both define same-named method
#[tokio::test]
async fn fuzz_bug05_requires_child_override_does_not_leak_into_parent_slot() {
    let output = baml_test!(
        r##"interface A {
    function foo(self) -> string { return "A" }
}
interface B requires A {
    function foo(self) -> string { return "B" }
}
class C {
    implements A {}
    implements B {
        function foo(self) -> string { return "B-override" }
    }
}
function main() -> string {
    let c = C {}
    let a: A = c
    return a.foo()
}
"##
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::String("A".into()));
}

/// Finding #6 [bad-error]: interface requires a class/enum/unknown type is silently accepted at declaration; error only fires (misleadingly) at class implements site
#[test]
fn fuzz_bug06_interface_requires_non_interface_errors_at_declaration() {
    assert_compile_error_contains(
        r##"
        class Foo { x: int }
        interface Bad requires Foo {}
        function main() -> bool { return true }
"##,
        "Foo",
    );
}

/// Finding #7 [wrong-result]: Interface type param T is unresolved in generic interface default method bodies
#[tokio::test]
async fn fuzz_bug07_generic_default_method_body_can_use_type_param() {
    let output = baml_test!(
        r##"interface Echo<T> {
    function echo(self, x: T) -> T {
        return x
    }
}

class IntEcho {
    implements Echo<int> {}
}

function main() -> int {
    let e = IntEcho {}
    return e.echo(42)
}
"##
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(42));
}

/// Finding #8 [wrong-result]: Generic class implementing single-T generic interface for both type params always dispatches to first implementor
#[tokio::test]
async fn fuzz_bug08_generic_class_dispatches_by_type_arg_to_correct_impl() {
    let output = baml_test!(
        r##"interface Getter<T> {
    function get(self) -> T
}

class Pair<L, R> {
    left: L
    right: R
    implements Getter<L> {
        function get(self) -> L { return self.left }
    }
    implements Getter<R> {
        function get(self) -> R { return self.right }
    }
}

function main() -> bool {
    let p: Pair<int, string> = Pair { left: 7, right: "seven" }
    let gr: Getter<string> = p
    let val = gr.get()
    return val == "seven"
}
"##
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

/// Finding #9 [wrongly-accepted]: Unqualified method call on class implementing same generic interface with different type args silently picks first impl (no E0121)
#[test]
fn fuzz_bug09_same_generic_iface_diff_typeargs_unqualified_call_ambiguous() {
    assert_compile_error_code(
        r##"interface Converter<T> {
    function convert(self) -> T
}

class MultiFormat {
    int_val: int
    str_val: string
    implements Converter<int> {
        function convert(self) -> int { return self.int_val }
    }
    implements Converter<string> {
        function convert(self) -> string { return self.str_val }
    }
}

function main() -> int {
    let m = MultiFormat { int_val: 42, str_val: "hello" }
    return m.convert()
}
"##,
        "E0121",
    );
}

/// Finding #10 [crash]: Generic function with interface type parameter Box<T> crashes at runtime with 'expected map, got instance' when dispatching methods
#[tokio::test]
async fn fuzz_bug10_generic_interface_as_function_param_dispatches() {
    let output = baml_test!(
        r##"interface Box<T> {
    function get(self) -> T
}

class IntBox {
    value: int
    implements Box<int> {
        function get(self) -> int { return self.value }
    }
}

// Function parameter type is Box<T> (generic interface)
function read<T>(b: Box<T>) -> T {
    return b.get()
}

function main() -> int {
    let b = IntBox { value: 55 }
    return read<int>(b)
}
"##
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(55));
}

/// Finding #11 [bad-error]: Error message for ambiguous generic interface field access suggests invalid fix (as<Box> without type args)
#[test]
fn fuzz_bug11_ambiguous_generic_field_error_includes_type_args() {
    assert_compile_error_contains(
        r##"interface Box<T> {
    value: T
}

class MultiBox {
    int_v: int
    str_v: string
    implements Box<int> { value as int_v }
    implements Box<string> { value as str_v }
}

function main() -> int {
    let b = MultiBox { int_v: 42, str_v: "hello" }
    return b.value
}
"##,
        "as<Box<int>>",
    );
}

/// Finding #12 [wrong-result]: Generic class Pair<L,R> always dispatches to the FIRST implements block for both Slot<L> and Slot<string> interface types
#[tokio::test]
async fn fuzz_bug12_generic_pair_dispatches_second_type_arg_correctly() {
    let output = baml_test!(
        r##"interface Slot<T> {
    function get(self) -> T
}

class GenPair<L, R> {
    left: L
    right: R
    implements Slot<L> {
        function get(self) -> L { return self.left }
    }
    implements Slot<R> {
        function get(self) -> R { return self.right }
    }
}

function main() -> bool {
    let p: GenPair<int, string> = GenPair { left: 42, right: "world" }
    let rl: Slot<int> = p
    let rr: Slot<string> = p
    let lv = rl.get()
    let rv = rr.get()
    // Expected: lv == 42 && rv == "world"
    // Actual:   lv == 42 && rv == 42  (second dispatch picks first impl block)
    return lv == 42 && rv == "world"
}
"##
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

/// Finding #13 [wrong-result]: Same dispatch bug affects field-link views: GenPair<L,R> with 'value as left' and 'value as right' returns wrong field for second type param
#[tokio::test]
async fn fuzz_bug13_generic_field_link_views_select_correct_type_arg() {
    let output = baml_test!(
        r##"interface Slot<T> {
    value: T
}

class GenPair<L, R> {
    left: L
    right: R
    implements Slot<L> {
        value as left
    }
    implements Slot<R> {
        value as right
    }
}

function main() -> bool {
    let p: GenPair<int, string> = GenPair { left: 7, right: "seven" }
    let i: Slot<int> = p
    let s: Slot<string> = p
    // Expected: i.value == 7 && s.value == "seven"
    // Actual: i.value == 7 && s.value == 7 (wrong impl selected)
    return i.value == 7 && s.value == "seven"
}
"##
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

/// Finding #14 [wrong-result]: Explicit .as<Slot<string>> projection on a generic Pair<int,string> also dispatches to the wrong block
#[tokio::test]
async fn fuzz_bug14_explicit_as_projection_selects_generic_type_arg() {
    let output = baml_test!(
        r##"interface Slot<T> {
    function get(self) -> T
}

class GenPair<L, R> {
    left: L
    right: R
    implements Slot<L> {
        function get(self) -> L { return self.left }
    }
    implements Slot<R> {
        function get(self) -> R { return self.right }
    }
}

function main() -> bool {
    let p: GenPair<int, string> = GenPair { left: 42, right: "world" }
    // Explicit .as<> projection should select the Slot<string> block
    let rv = p.as<Slot<string>>.get()
    return rv == "world"
}
"##
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

/// Finding #15 [crash]: Generic interface default method crashes with 'expected map, got instance' when calling self.method() through an interface-typed variable
#[tokio::test]
async fn fuzz_bug15_generic_default_method_self_call_through_interface_var() {
    let output = baml_test!(
        r##"interface Container<T> {
    function size(self) -> int
    function describe_with_self_call(self) -> string {
        let n = self.size()   // calling another interface method on self
        return "ok"
    }
}

class IntBox {
    items: int[]
    implements Container<int> {
        function size(self) -> int { return self.items.length() }
    }
}

function main() -> string {
    let b = IntBox { items: [1, 2, 3] }
    let c: Container<int> = b
    return c.describe_with_self_call()
}
"##
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("ok".into())
    );
}

/// Finding #16 [bad-error]: Type parameter T from a generic interface is not in scope in default method signatures and bodies (E0002 'unresolved type: T')
#[tokio::test]
async fn fuzz_bug16_generic_interface_type_param_in_scope_in_default_method() {
    let output = baml_test!(
        r##"interface Container<T> {
    function get(self) -> T           // required method - T is fine here
    function get_or(self, fallback: T) -> T {   // default method - T is NOT in scope!
        return self.get()
    }
}

class IntBox {
    value: int
    implements Container<int> {
        function get(self) -> int { return self.value }
    }
}

function main() -> bool {
    return true
}
"##
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

/// Finding #17 [wrong-result]: Inherited default method (not overridden) inaccessible via class-typed variable
#[tokio::test]
async fn fuzz_bug17_inherited_default_method_callable_on_class_var() {
    let output = baml_test!(
        r##"interface Greetable {
    function greet(self) -> string {
        return "Hello!"
    }
}
class Greeter {
    implements Greetable {}  // empty block - inherits default
}
function main() -> string {
    let g = Greeter {}
    return g.greet()  // should work - inherits default
}
"##
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Hello!".into())
    );
}

/// Finding #18 [wrong-result]: Class method shadowed by aliased interface field view — calling class method fails with misleading interface-field error
#[tokio::test]
async fn fuzz_bug18_class_method_preferred_over_aliased_field_view_on_call() {
    let output = baml_test!(
        r##"interface Named {
    name: string
}
class Person {
    _name: string
    implements Named {
        name as _name
    }
    function name(self) -> string { return "method:" + self._name }
}
function main() -> string {
    let p = Person { _name: "Ada" }
    return p.name()  // Should call the class method
}
"##
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("method:Ada".into())
    );
}

/// Finding #19 [bad-error]: E0116 type-mismatch error on aliased field reports the class field name as the interface's requirement
#[test]
fn fuzz_bug19_aliased_field_type_mismatch_error_names_interface_field() {
    assert_compile_error_contains(
        r##"interface Named {
    name: string
}
class Person {
    name_count: int
    implements Named {
        name as name_count
    }
}
function main() -> string {
    return "unreachable"
}
"##,
        "`name`",
    );
}

/// Finding #20 [wrong-result]: Field dispatch via .as<B>.field resolves to parent interface's field view when B requires A and both declare same field name
#[tokio::test]
async fn fuzz_bug20_as_projection_field_uses_own_view_in_requires_chain() {
    let output = baml_test!(
        r##"interface A {
    label: string
}
interface B requires A {
    label: string
}
class D {
    a_label: string
    b_label: string
    implements A { label as a_label }
    implements B { label as b_label }
}
function main() -> string {
    let d = D { a_label: "A_val", b_label: "B_val" }
    let a_val = d.as<A>.label
    let b_val = d.as<B>.label
    return a_val + "|" + b_val
}
"##
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("A_val|B_val".into())
    );
}

/// Finding #21 [wrong-result]: Interface-typed variable field access resolves to parent interface field when requires chain has same field name
#[tokio::test]
async fn fuzz_bug21_interface_param_field_uses_own_view_in_requires_chain() {
    let output = baml_test!(
        r##"interface A {
    label: string
}
interface B requires A {
    label: string
}
class D {
    a_label: string
    b_label: string
    implements A { label as a_label }
    implements B { label as b_label }
}
function get_from_b(b: B) -> string {
    return b.label
}
function get_from_a(a: A) -> string {
    return a.label
}
function main() -> string {
    let d = D { a_label: "A_val", b_label: "B_val" }
    return get_from_a(d) + "|" + get_from_b(d)
}
"##
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("A_val|B_val".into())
    );
}

/// Finding #22 [nit]: Old x.Interface.method() hint for generic interface omits type arguments
#[test]
fn fuzz_bug22_old_projection_syntax_hint_includes_type_args() {
    assert_compile_error_contains(
        r##"interface Container<T> {
    function get(self) -> T
}
class IntBox {
    value: int
    implements Container<int> {
        function get(self) -> int { return self.value }
    }
}
function main() -> int {
    let b = IntBox { value: 42 }
    return b.Container.get()
}
"##,
        "Container<int>",
    );
}

/// Finding #23 [wrong-result]: Default-inherited method not accessible via unqualified call on concrete class type
#[tokio::test]
async fn fuzz_bug23_default_inherited_method_unqualified_call_on_class() {
    let output = baml_test!(
        r##"interface Speaker {
    function speak(self) -> string {
        return "default speech"
    }
}

class Thing {
    implements Speaker {}
}

function main() -> string {
    let t = Thing {}
    return t.speak()
}
"##
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("default speech".into())
    );
}

/// Finding #24 [wrong-result]: Generic interface type parameter T is unresolved in default method signatures
#[tokio::test]
async fn fuzz_bug24_generic_interface_type_param_in_default_method_signature() {
    let output = baml_test!(
        r##"interface Container<T> {
    // Default method using the interface's type parameter `T` in both its
    // signature and body — `T` must be in scope here.
    function identity(self, x: T) -> T {
        return x
    }
    function get(self) -> T  // required
}

class IntBag {
    items: int[]
    implements Container<int> {
        function get(self) -> int { return self.items[0] }
    }
}

function main() -> int {
    let b: Container<int> = IntBag { items: [42, 1, 2] }
    return b.get()
}
"##
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(42));
}

/// Finding #25 [bad-error]: Two default methods with same name give E0007 (method not found) instead of E0121 (ambiguous)
#[test]
fn fuzz_bug25_two_same_named_default_methods_are_ambiguous() {
    assert_compile_error_code(
        r##"interface Alpha {
    function tag(self) -> string { return "alpha" }
}
interface Beta {
    function tag(self) -> string { return "beta" }
}

class Both {
    implements Alpha {}
    implements Beta {}
}

function main() -> string {
    let x = Both {}
    return x.tag()
}
"##,
        "E0121",
    );
}

/// Finding #26 [wrong-result]: One default + one required method with same name: unqualified call silently picks required without E0121
#[test]
fn fuzz_bug26_default_plus_required_same_name_is_ambiguous() {
    assert_compile_error_code(
        r##"interface WithDefault {
    function process(self) -> string { return "DEFAULT" }
}
interface WithRequired {
    function process(self) -> string
}

class Impl {
    implements WithDefault {}
    implements WithRequired {
        function process(self) -> string { return "REQUIRED" }
    }
}

function main() -> string {
    let x = Impl {}
    return x.process()  // Expected: E0121; Actual: compiles and returns "REQUIRED"
}
"##,
        "E0121",
    );
}

/// Finding #27 [wrong-result]: Same generic interface implemented with different type args: unqualified call silently picks first by declaration order instead of raising E0121
#[test]
fn fuzz_bug27_same_generic_iface_diff_typeargs_no_receiver_is_ambiguous() {
    assert_compile_error_code(
        r##"interface Converter<T> {
    function convert(self) -> T
}

class Multi {
    implements Converter<int> {
        function convert(self) -> int { return 42 }
    }
    implements Converter<string> {
        function convert(self) -> string { return "hello" }
    }
}

function main() -> int {
    let m = Multi {}
    return m.convert()  // picks Converter<int> silently
}
"##,
        "E0121",
    );
}

/// Finding #28 [bad-error]: E0007 with misleading suggestion for generic-interface field access: suggests raw `as<Slot>` which itself fails
#[test]
fn fuzz_bug28_ambiguous_generic_field_error_lists_type_args() {
    assert_compile_error_contains(
        r##"interface Slot<T> {
    value: T
}

class Pair {
    int_value: int
    str_value: string
    implements Slot<int> { value as int_value }
    implements Slot<string> { value as str_value }
}

function main() -> string {
    let p = Pair { int_value: 1, str_value: "hi" }
    return p.value
}
"##,
        "Slot<int>",
    );
}

/// Findings #29/#30: the canonical `let d: Dog =>` match-binding form narrows an
/// interface to a concrete class and binds it. (The no-`let` `d: Dog =>` form is
/// intentionally not valid syntax; the existing
/// `match_narrows_interface_to_concrete_class` test was corrected to use `let`.)
#[tokio::test]
async fn fuzz_bug29_match_binding_form_narrows_to_concrete() {
    let output = baml_test!(
        r##"interface Animal {
    function speak(self) -> string
}
class Dog {
    breed: string
    implements Animal {
        function speak(self) -> string { return "Woof!" }
    }
}

function describe(a: Animal) -> string {
    return match (a) {
        let d: Dog => d.breed
        _ => "other"
    }
}

function main() -> string {
    let d = Dog { breed: "Retriever" }
    return describe(d)
}
"##
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Retriever".into())
    );
}

/// Finding #31 [crash]: VM crash when calling method on `Dog | Cat` union type produced by match expression arms
#[tokio::test]
async fn fuzz_bug31_method_call_on_match_union_result_does_not_crash() {
    let output = baml_test!(
        r##"interface Animal {
    function speak(self) -> string
}
class Dog {
    implements Animal {
        function speak(self) -> string { return "Woof!" }
    }
}
class Cat {
    implements Animal {
        function speak(self) -> string { return "Meow!" }
    }
}

function main() -> string {
    let b = true
    let result = match (b) {
        true => Dog {}
        false => Cat {}
    }
    // result is Dog | Cat union - calling any method crashes the VM
    return result.speak()
}
"##
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Woof!".into())
    );
}

/// Finding #32 [wrong-result]: Interface-typed variable cannot be coerced to optional interface (`Animal -> Animal?`), while `int -> int?` and `Dog -> Dog?` both work
#[tokio::test]
async fn fuzz_bug32_interface_value_coerces_to_optional_interface() {
    let output = baml_test!(
        r##"interface Animal {
    function speak(self) -> string
}
class Dog {
    breed: string
    implements Animal {
        function speak(self) -> string { return "Woof!" }
    }
}

function main() -> bool {
    let d: Animal = Dog { breed: "Lab" }
    let opt: Animal? = d   // should work: Animal is a subtype of Animal?
    return opt != null
}
"##
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

/// Finding #33 [wrong-result]: Returning concrete class from function with `-> Animal?` return type fails even though Dog is a subtype of Animal
#[tokio::test]
async fn fuzz_bug33_return_implementor_from_optional_interface_function() {
    let output = baml_test!(
        r##"interface Animal {
    function speak(self) -> string
}
class Dog {
    breed: string
    implements Animal {
        function speak(self) -> string { return "Woof!" }
    }
}

// Should work: Dog <: Animal <: Animal?
function maybe_dog(want_dog: bool) -> Animal? {
    if want_dog {
        return Dog { breed: "Lab" }
    }
    return null
}

function main() -> bool {
    let r = maybe_dog(true)
    let n = maybe_dog(false)
    return r != null && n == null
}
"##
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

/// Finding #34 [wrong-result]: implements() ignores generic type arguments — IntBox.implements(Box<string>) returns true
#[tokio::test]
async fn fuzz_bug34_reflect_implements_respects_generic_type_args() {
    let output = baml_test!(
        r##"interface Box<T> {
    function get(self) -> T
}
class IntBox {
    implements Box<int> {
        function get(self) -> int { return 1 }
    }
}

function main() -> bool {
    // IntBox only implements Box<int>, NOT Box<string>
    // Expected: false. Actual: true (wrong!)
    return reflect.type_of<IntBox>().implements(reflect.type_of<Box<string>>())
}
"##
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(false));
}

/// Finding #35 [wrong-result]: implemented_by() also ignores generic type arguments — Box<string>.implemented_by(IntBox) returns true
#[tokio::test]
async fn fuzz_bug35_reflect_implemented_by_respects_generic_type_args() {
    let output = baml_test!(
        r##"interface Box<T> {
    function get(self) -> T
}
class IntBox {
    implements Box<int> {
        function get(self) -> int { return 1 }
    }
}

function main() -> bool {
    // Box<string>.implemented_by(IntBox) should be false
    // IntBox only implements Box<int>
    return reflect.type_of<Box<string>>().implemented_by(reflect.type_of<IntBox>())
}
"##
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(false));
}

/// Finding #36 [wrong-result]: implementors() on a generic interface ignores type args — Box<int>.implementors() returns both IntBox and StringBox
#[tokio::test]
async fn fuzz_bug36_reflect_implementors_respects_generic_type_args() {
    let output = baml_test!(
        r##"interface Box<T> {
    function get(self) -> T
}
class IntBox {
    implements Box<int> {
        function get(self) -> int { return 1 }
    }
}
class StringBox {
    implements Box<string> {
        function get(self) -> string { return "hello" }
    }
}

function main() -> int {
    // Box<int>.implementors() should return [IntBox] (length 1)
    // But actually returns both IntBox and StringBox (length 2)
    return reflect.type_of<Box<int>>().implementors().length()
}
"##
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(1));
}

/// Finding #37 [wrong-result]: implementors() returned items in a
/// nondeterministic order with 3+ implementors. They are now in a stable
/// lexicographic order by qualified name (`A,B,C`).
#[tokio::test]
async fn fuzz_bug37_reflect_implementors_deterministic_order() {
    let output = baml_test!(
        r##"interface I {}
class A { implements I {} }
class B { implements I {} }
class C { implements I {} }

function main() -> string {
    let impls = reflect.type_of<I>().implementors()
    // Lexicographic by name: A,B,C
    return impls[0].to_string() + "," + impls[1].to_string() + "," + impls[2].to_string()
}
"##
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("A,B,C".into())
    );
}

/// Finding #38 [wrong-result]: E0096 false positive: throwing a class that implements the declared throws interface is rejected
#[tokio::test]
async fn fuzz_bug38_throw_subtype_of_declared_throws_interface_is_allowed() {
    let output = baml_test!(
        r##"interface IError {
    function describe(self) -> string
}

class NetworkError {
    msg: string
    implements IError {
        function describe(self) -> string { return "network: " + self.msg }
    }
}

class DatabaseError {
    msg: string
    implements IError {
        function describe(self) -> string { return "db: " + self.msg }
    }
}

interface DataFetcher {
    function fetch(self, key: string) -> string throws IError
}

class MyFetcher {
    implements DataFetcher {
        function fetch(self, key: string) -> string throws IError {
            if key == "net" {
                throw NetworkError { msg: "connection refused" }
            }
            return "data:" + key
        }
    }
}

function main() -> bool {
    return true
}
"##
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

/// Finding #39 [wrong-result]: catch-by-interface-type pattern silently fails to match at runtime when throws type is an interface
#[tokio::test]
async fn fuzz_bug39_catch_by_interface_pattern_matches_implementor() {
    let output = baml_test!(
        r##"interface IError {
    function describe(self) -> string
}

class ConcreteErr {
    msg: string
    implements IError {
        function describe(self) -> string { return self.msg }
    }
}

function risky() -> string throws IError {
    let e: IError = ConcreteErr { msg: "problem" }
    throw e
}

function main() -> string {
    return risky() catch (e) {
        let caught: IError => "interface-caught: " + caught.describe()
        _ => "wildcard-caught"
    }
}
"##
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("interface-caught: problem".into())
    );
}

/// Finding #40 [wrong-result]: Concrete class not assignable to `Interface | OtherType` union even when class implements Interface
#[tokio::test]
async fn fuzz_bug40_implementor_assignable_to_interface_union() {
    let output = baml_test!(
        r##"interface Animal {
    function speak(self) -> string
}
class Dog {
    implements Animal {
        function speak(self) -> string { return "Woof!" }
    }
}

function describe(x: Animal | string) -> string {
    return "ok"
}

function main() -> string {
    return describe(Dog {})
}
"##
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("ok".into())
    );
}

/// Finding #41 [wrong-result]: Exhaustiveness checker wrongly marks `null` arm as unreachable when matching `Interface?`; runtime silently matches null as the interface type
#[tokio::test]
async fn fuzz_bug41_optional_interface_match_null_arm_reachable() {
    let output = baml_test!(
        r##"interface Animal {
    function speak(self) -> string
}
class Dog {
    implements Animal {
        function speak(self) -> string { return "Woof!" }
    }
}

// Should require null arm for exhaustiveness; without it should be a compile error.
// With it, null arm should NOT be unreachable.
function maybe_speak(a: Animal?) -> string {
    return match (a) {
        let animal: Animal => animal.speak()
        null => "silent"
    }
}

function main() -> string {
    return maybe_speak(null)
}
"##
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("silent".into())
    );
}

/// Finding #42 [wrong-result]: Default interface method (inherited via empty implements block) not callable on concrete class type — only on interface-typed variables
#[tokio::test]
async fn fuzz_bug42_default_method_callable_on_concrete_class_type() {
    let output = baml_test!(
        r##"interface Printable {
    function print(self) -> string { return "printable" }
}

class Widget {
    implements Printable {}
}

function main() -> string {
    let w = Widget {}
    // Widget inherits Printable.print via empty implements block
    return w.print()
}
"##
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("printable".into())
    );
}

/// Finding #43 [crash]: VM crash `expected map, got instance` when calling an interface method inside a generic function whose parameter type is `Interface<T>` with unbound T
#[tokio::test]
async fn fuzz_bug43_generic_fn_with_generic_interface_param_dispatches() {
    let output = baml_test!(
        r##"interface Producer<T> {
    function produce(self) -> T
}

class IntProducer {
    val: int
    implements Producer<int> {
        function produce(self) -> int { return self.val }
    }
}

// T is unconstrained but used as the type arg in Producer<T>
function get_value<T>(p: Producer<T>) -> T {
    return p.produce()
}

function main() -> int {
    let p = IntProducer { val: 42 }
    return get_value<int>(p)
}
"##
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(42));
}

/// Finding #44 [wrong-result]: Generic interface default method body cannot reference type parameter T — unresolved type error
#[tokio::test]
async fn fuzz_bug44_generic_interface_default_method_body_uses_type_param() {
    let output = baml_test!(
        r##"interface Wrapper<T> {
    function wrap(self, x: T) -> T {
        return x
    }
}

class IntWrapper {
    implements Wrapper<int> {}
}

function main() -> int {
    let w = IntWrapper {}
    return w.wrap(42)
}
"##
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(42));
}

/// Finding #45 [wrong-result]: Union type `Interface | OtherType` exhaustiveness checking treats union as only the interface — other arms wrongly flagged unreachable
#[tokio::test]
async fn fuzz_bug45_interface_union_match_arms_all_reachable() {
    let output = baml_test!(
        r##"interface Animal {
    function speak(self) -> string
}
class Dog {
    implements Animal {
        function speak(self) -> string { return "Woof!" }
    }
}

function describe(x: Animal | string) -> string {
    return match (x) {
        let a: Animal => a.speak()
        let s: string => s
    }
}

function main() -> string {
    let a: Animal = Dog {}
    return describe(a)
}
"##
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Woof!".into())
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Group R: namespace-resolution regression fixes (wf2 re-verification).
//
// Three genuine defects survived re-verification under the namespace
// resolution rule (`root.` is the absolute cross-namespace form). Each is
// pinned here.
// ─────────────────────────────────────────────────────────────────────────────

// ── #7: `requires <unknown>` must be E0112 (unknown name), not E0133 ─────────

#[test]
fn requires_unknown_name_is_unknown_interface_not_non_interface() {
    // `requires DoesNotExist` names nothing at all. The diagnostic must say
    // "no interface with that name is in scope" (E0112, like `implements`),
    // not "is not an interface" (E0133) — the latter wrongly implies the
    // symbol exists with the wrong kind.
    let errors = collect_compile_errors(
        r#"
        interface Person requires DoesNotExist {
            name: string
        }
        "#,
    );
    assert!(
        errors.iter().any(|e| e.starts_with("[E0112]")),
        "expected an E0112 unknown-interface error, got:\n  {}",
        errors.join("\n  ")
    );
    assert!(
        !errors.iter().any(|e| e.starts_with("[E0133]")),
        "must NOT emit E0133 (`is not an interface`) for a name that does not \
         exist at all; got:\n  {}",
        errors.join("\n  ")
    );
}

#[test]
fn requires_real_non_interface_is_still_non_interface_error() {
    // Regression guard for the fix above: `requires` a *real* class (a
    // wrong-kind target, not an unknown name) must still be E0133.
    assert_compile_error_code(
        r#"
        class RealClass { x: int }
        interface Person requires RealClass {
            name: string
        }
        "#,
        "E0133",
    );
}

// ── #4: an unresolvable type argument in an `implements` clause is E0002 ──────

#[test]
fn unresolvable_type_arg_in_implements_clause_is_error() {
    // `implements Container<DoesNotExist>` has a bad generic type argument.
    // It must be rejected with E0002 (unresolved type) exactly like the same
    // name in field position — not silently swallowed (which let the program
    // compile and run with the implements relation never registering).
    assert_compile_error_code(
        r#"
        interface Container<T> {
            function size(self) -> int
        }
        class Box {
            items: int[]
            implements Container<DoesNotExist> {
                function size(self) -> int { return 0 }
            }
        }
        "#,
        "E0002",
    );
}

#[test]
fn resolvable_type_arg_in_implements_clause_is_ok() {
    // Control for the test above: a valid concrete type argument compiles.
    assert_no_interface_errors(
        r#"
        interface Container<T> {
            function size(self) -> int
        }
        class Box {
            items: int[]
            implements Container<int> {
                function size(self) -> int { return 0 }
            }
        }
        "#,
    );
}

// ── #6: ambiguous same-name interface methods across namespaces qualify ──────

#[test]
fn ambiguous_method_across_namespaces_uses_qualified_interface_names() {
    // `Parrot` implements `zoo.Animal` and `farm.Animal`, two distinct
    // interfaces that share the simple name `Animal`. The E0121 ambiguity
    // diagnostic must qualify them (`zoo.Animal` / `farm.Animal`) so they are
    // distinguishable — and the suggested `as<…>` fixes must name the
    // namespace-qualified form (which actually compiles), not the bare,
    // identical, uncompilable `as<Animal>`.
    let files = &[
        (
            "main.baml",
            r#"
            class Parrot {
                implements zoo.Animal {
                    function speak(self) -> string { return "squawk" }
                }
                implements farm.Animal {
                    function speak(self) -> string { return "cluck" }
                }
            }
            function main() -> string {
                let p = Parrot {}
                return p.speak()
            }
            "#,
        ),
        (
            "ns_zoo/zoo.baml",
            r#"interface Animal { function speak(self) -> string }"#,
        ),
        (
            "ns_farm/farm.baml",
            r#"interface Animal { function speak(self) -> string }"#,
        ),
    ];
    let errors = collect_compile_errors_multi(files);
    let e0121: Vec<_> = errors.iter().filter(|e| e.starts_with("[E0121]")).collect();
    assert!(
        !e0121.is_empty(),
        "expected an E0121 ambiguous-method error, got:\n  {}",
        errors.join("\n  ")
    );
    let msg = e0121[0];
    for needle in [
        "zoo.Animal",
        "farm.Animal",
        "as<zoo.Animal>",
        "as<farm.Animal>",
    ] {
        assert!(
            msg.contains(needle),
            "E0121 message must contain {needle:?} (qualified, compilable), got:\n  {msg}"
        );
    }
}

#[test]
fn ambiguous_method_namespace_qualified_fix_compiles() {
    // The fix the E0121 message suggests — `p.as<zoo.Animal>.speak()` — must
    // actually resolve and compile cleanly.
    let files = &[
        (
            "main.baml",
            r#"
            class Parrot {
                implements zoo.Animal {
                    function speak(self) -> string { return "squawk" }
                }
                implements farm.Animal {
                    function speak(self) -> string { return "cluck" }
                }
            }
            function main() -> string {
                let p = Parrot {}
                return p.as<zoo.Animal>.speak()
            }
            "#,
        ),
        (
            "ns_zoo/zoo.baml",
            r#"interface Animal { function speak(self) -> string }"#,
        ),
        (
            "ns_farm/farm.baml",
            r#"interface Animal { function speak(self) -> string }"#,
        ),
    ];
    assert_no_compile_errors_multi(files);
}

// ═══════════════════════════════════════════════════════════════════════════
// wf3 design-audit regression suite — derived from a 12-area workflow sweep.
//
// Each program was authored as a well-formed, `root.`-qualified BAML snippet and
// run through `baml-cli`; findings are recorded in `_plan/wf3/FINDINGS.md`, and
// every test below names the originating `_plan/wf3/<area>/<file>` repro.
//
// Like the `fuzz_*` suite above, the bug / desired-direction tests pin the
// CORRECT behavior and are expected to FAIL on current code (reproducing the
// defect) and to start passing as each is fixed. Tests suffixed `_pins` pin
// behavior that is correct today (sometimes subtle or surprising) so a
// regression is caught.
// ═══════════════════════════════════════════════════════════════════════════

// ── Crashes (type-checker-accepted code that panics the VM) ──────────────────

/// wf3 #1 [crash]: generic `Sub<T> requires Base<T>` whose default method calls
/// the parent's required method via `self` crashes the VM when dispatched
/// through an interface-typed var. `_plan/wf3/generics-core/gen_requires_generic.baml`
#[tokio::test]
async fn wf3_generic_requires_chain_default_delegation_runtime() {
    let output = baml_test!(
        r#"
        interface Base<T> {
            function base_get(self) -> T
        }
        interface Sub<T> requires Base<T> {
            function sub_get(self) -> T {
                return self.base_get()
            }
        }
        class IntThing {
            value: int
            implements Base<int> {
                function base_get(self) -> int { return self.value }
            }
            implements Sub<int> {}
        }
        function main() -> int {
            let t = IntThing { value: 77 }
            let s: Sub<int> = t
            return s.sub_get()
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(77));
}

/// wf3 #2 [crash]: chained Form2 blanket impls (`Loud for T where T: Printable`,
/// `Printable for T where T: Named`) crash the VM.
/// `_plan/wf3/generics-bounds-blanket/p8b_blanket_on_blanket_min.baml`
#[tokio::test]
async fn wf3_blanket_on_blanket_chain_runtime() {
    let output = baml_test!(
        r#"
        interface Named { name: string }
        interface Printable { function display(self) -> string }
        interface Loud { function shout(self) -> string }
        class Person { name: string  implements Named {} }
        implements<T extends Named> Printable for T {
            function display(self) -> string { return self.name }
        }
        implements<T extends Printable> Loud for T {
            function shout(self) -> string { return "loud" }
        }
        function main() -> string {
            let p: Loud = Person { name: "Ada" }
            return p.shout()
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::String("loud".into()));
}

/// wf3 #3 [crash/unsound]: a phantom impl type param
/// (`implements<T> Tagged<T> for Holder` where `T` appears only in the interface
/// args) must be rejected at compile time — today it compiles, is accepted as
/// `Tagged<int|string|bool>`, and crashes the VM.
/// `_plan/wf3/generics-bounds-blanket/p13c_phantom_single.baml`
#[test]
fn wf3_phantom_impl_type_param_is_rejected() {
    let errors = collect_compile_errors(
        r#"
        interface Tagged<T> { function tag(self) -> string }
        class Holder { v: int }
        implements<T> Tagged<T> for Holder {
            function tag(self) -> string { return "h" }
        }
        function main() -> string {
            let a: Tagged<int> = Holder { v: 1 }
            return a.tag()
        }
        "#,
    );
    assert!(
        !errors.is_empty(),
        "an unconstrained phantom type param in an `implements` (`T` only in the \
         interface args) is unsound and must be a compile error; got none"
    );
}

/// wf3 #4 [crash]: `default.<field>` type-checks as `string` but reads `null`,
/// then `string + any` crashes the VM. Expected: `default.name` resolves to the
/// field like `self.name`/`self.as<Named>.name` do (all read "Bob"). (If the
/// language instead restricts `default` to call position — see
/// `wf3_default_as_bare_value_is_rejected` — this should become a compile error,
/// not a crash.) `_plan/wf3/default-methods/p6d_default_vs_self_field.baml`
#[tokio::test]
async fn wf3_default_field_access_does_not_crash() {
    let output = baml_test!(
        r#"
        interface Named {
            name: string
            function describe(self) -> string { return "x" }
        }
        class P {
            name: string
            implements Named {
                function describe(self) -> string {
                    return "default.name=[" + default.name + "] self.name=[" + self.name + "] as=[" + self.as<Named>.name + "]"
                }
            }
        }
        function main() -> string {
            let p = P { name: "Bob" }
            return p.describe()
        }
        "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("default.name=[Bob] self.name=[Bob] as=[Bob]".into())
    );
}

// ── High-severity wrong-result / soundness ───────────────────────────────────

/// wf3 #5 [high]: a generic interface's default method calling `self.get()` must
/// dispatch to the interface VIEW it was reached through, not the first impl
/// block. Through `Slot<string>` it must return "seven", not 7.
/// `_plan/wf3/generics-core/gen_pair_default_selfcall.baml`
#[tokio::test]
async fn wf3_generic_default_method_self_call_respects_interface_view_runtime() {
    let output = baml_test!(
        r#"
        interface Slot<T> {
            function get(self) -> T
            function describe(self) -> T {
                return self.get()
            }
        }
        class GenPair<L, R> {
            left: L
            right: R
            implements Slot<L> {
                function get(self) -> L { return self.left }
            }
            implements Slot<R> {
                function get(self) -> R { return self.right }
            }
        }
        function main() -> string {
            let p: GenPair<int, string> = GenPair { left: 7, right: "seven" }
            let s: Slot<string> = p
            return s.describe()
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::String("seven".into()));
}

/// wf3 #6 [high]: `reflect.type_of<Box<U>>()` inside a generic fn must substitute
/// the outer type param `U` into the interface arg — rendering `Box<int>`, not
/// `Box<void>`. `_plan/wf3/generics-reflection/gen_reflect_naked_vs_wrapped.baml`
#[tokio::test]
async fn wf3_reflect_type_of_wrapped_generic_substitutes_param_runtime() {
    let output = baml_test!(
        r#"
        interface Box<T> {
            function get(self) -> T
        }
        function names<U>() -> string {
            let naked = reflect.type_of<U>().to_string()
            let wrapped = reflect.type_of<Box<U>>().to_string()
            return "naked=" + naked + " wrapped=" + wrapped
        }
        function main() -> string {
            return names<int>()
        }
        "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("naked=int wrapped=Box<int>".into())
    );
}

/// wf3 #7 [high]: consequence of #6 — `implemented_by`/`implementors` of
/// `Box<U>` are wrong inside a generic fn because the param substitution is
/// dropped. `_plan/wf3/generics-reflection/gen_reflect_boxT.baml`
#[tokio::test]
async fn wf3_reflect_implemented_by_generic_arg_substitution_runtime() {
    let output = baml_test!(
        r#"
        interface Box<T> {
            function get(self) -> T
        }
        class IntBox {
            implements Box<int> {
                function get(self) -> int { return 1 }
            }
        }
        class StringBox {
            implements Box<string> {
                function get(self) -> string { return "hi" }
            }
        }
        function box_impls_intbox<T>() -> bool {
            return reflect.type_of<Box<T>>().implemented_by(reflect.type_of<IntBox>())
        }
        function box_impls_stringbox<T>() -> bool {
            return reflect.type_of<Box<T>>().implemented_by(reflect.type_of<StringBox>())
        }
        function main() -> bool {
            return box_impls_intbox<int>()
                && box_impls_intbox<string>() == false
                && box_impls_stringbox<string>()
                && box_impls_stringbox<int>() == false
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

/// wf3 #8 [high/soundness]: a `-> Self` method in `Box`'s implements block has
/// `Self = Box`; returning a `Cup` must be a compile error. Today it compiles and
/// a `Box`-typed value backed by a runtime `Cup` reads a non-existent field.
/// `_plan/wf3/self-types/self_wrong_class_field_access.baml`
#[test]
fn wf3_self_return_wrong_concrete_class_is_rejected() {
    let errors = collect_compile_errors(
        r#"
        interface Cloneable {
            function clone(self) -> Self
        }
        class Box {
            boxField: int
            implements Cloneable {
                function clone(self) -> Self {
                    return Cup { cupField: 99 }
                }
            }
        }
        class Cup {
            cupField: int
            implements Cloneable {
                function clone(self) -> Self {
                    return Cup { cupField: self.cupField }
                }
            }
        }
        function main() -> int {
            let b = Box { boxField: 1 }
            let c = b.clone()
            return c.boxField
        }
        "#,
    );
    assert!(
        !errors.is_empty(),
        "returning a different concrete class (`Cup`) from a `Box` block's \
         `-> Self` method must be rejected — `Self` is `Box` there; got no errors"
    );
}

/// wf3 #9 [high]: `Self` in an interface FIELD type produces contradictory
/// diagnostics. With the concrete class field (`next: LinkedItem?`), E0116
/// demands the field be `Self?`; but following that advice (`next: Self?`)
/// errors E0002 `unresolved type: Self`. The fix must break this loop — either
/// accept the concrete field as satisfying `Self?`, or reject `Self`-in-field
/// cleanly without pointing at the unsatisfiable `Self?` form.
/// `_plan/wf3/self-types/self_in_field.baml`
#[test]
fn wf3_self_in_field_type_has_no_contradictory_diagnostics() {
    // Variant A: class declares the field with its concrete type.
    let errors_concrete = collect_compile_errors(
        r#"
        interface Node {
            value: int
            next: Self?
        }
        class LinkedItem {
            value: int
            next: LinkedItem?
            implements Node {}
        }
        function main() -> int {
            let tail = LinkedItem { value: 2, next: null }
            let head = LinkedItem { value: 1, next: tail }
            return match (head.next) {
                let n: LinkedItem => n.value,
                _ => -1,
            }
        }
        "#,
    );
    // Variant B: class follows the E0116 advice and writes `Self?` verbatim.
    let errors_self = collect_compile_errors(
        r#"
        interface Node {
            value: int
            next: Self?
        }
        class LinkedItem {
            value: int
            next: Self?
            implements Node {}
        }
        function main() -> int { return 0 }
        "#,
    );
    let advice_is_self_opt = errors_concrete
        .iter()
        .any(|e| e.starts_with("[E0116]") && e.contains("Self?"));
    let self_opt_is_unresolved = errors_self
        .iter()
        .any(|e| e.contains("unresolved type: Self"));
    assert!(
        !(advice_is_self_opt && self_opt_is_unresolved),
        "contradictory `Self`-in-field diagnostics: E0116 told the class to use \
         `Self?`, but writing `Self?` is itself E0002 `unresolved type: Self`.\n\
         concrete-field errors:\n  {}\nself?-field errors:\n  {}",
        errors_concrete.join("\n  "),
        errors_self.join("\n  ")
    );
}

// ── Medium-severity ──────────────────────────────────────────────────────────

/// wf3 #10 [medium]: when an ambiguous method call sits INSIDE a namespace, the
/// E0121 `as<...>` suggestion must be `root.`-qualified so it actually resolves
/// from that namespace — a bare `as<zoo.Animal>` reads as `birds.zoo.Animal`.
/// `_plan/wf3/namespace-collisions/ambig_inside_via_method`
#[test]
fn wf3_ambiguous_method_suggestion_resolvable_inside_namespace() {
    let files = &[
        (
            "main.baml",
            r#"function main() -> string { let p = birds.Parrot {} return p.run() }"#,
        ),
        (
            "ns_birds/birds.baml",
            r#"
            class Parrot {
                implements root.zoo.Animal { function speak(self) -> string { return "squawk" } }
                implements root.farm.Animal { function speak(self) -> string { return "cluck" } }
                function run(self) -> string { return self.speak() }
            }
            "#,
        ),
        (
            "ns_zoo/zoo.baml",
            r#"interface Animal { function speak(self) -> string }"#,
        ),
        (
            "ns_farm/farm.baml",
            r#"interface Animal { function speak(self) -> string }"#,
        ),
    ];
    assert_compile_error_contains_multi(files, "root.zoo.Animal");
}

/// wf3 #11 [medium]: a single method declared once but transitively required by
/// several interfaces (a pure diamond, no overrides) must NOT be flagged
/// ambiguous on a bare call. `_plan/wf3/requires-diamond/p2_diamond_base_method.baml`
#[tokio::test]
async fn wf3_pure_diamond_single_method_dispatches_runtime() {
    let output = baml_test!(
        r#"
        interface Base { function id(self) -> string { return "base" } }
        interface Left requires Base {}
        interface Right requires Base {}
        class D {
            implements Base {}
            implements Left {}
            implements Right {}
        }
        function main() -> string {
            let d = D {}
            return d.id()
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::String("base".into()));
}

/// wf3 #11b [medium]: same defect across namespaces — `W` provides `f` once (via
/// `root.a.Base`) but `requires`-reachability reports it from multiple
/// interfaces and rejects the bare call. `_plan/wf3/namespace-resolution/p3c_root_ok`
#[test]
fn wf3_pure_diamond_single_method_across_namespaces_no_false_ambiguity() {
    let files = &[
        (
            "main.baml",
            r#"function main() -> string { let w = b.W {} return w.f() }"#,
        ),
        ("ns_a/a.baml", r#"interface Base { function f(self) -> string }"#),
        (
            "ns_b/b.baml",
            r#"
            interface Derived requires root.a.Base {}
            class W {
                implements root.b.Derived {}
                implements root.a.Base { function f(self) -> string { return "from-base" } }
            }
            "#,
        ),
    ];
    assert_no_compile_errors_multi(files);
}

/// wf3 #11c [medium]: three-level requires chain (`C requires B requires A`),
/// `f` declared once on `A` — bare `w.f()` must resolve, not E0121.
/// `_plan/wf3/namespace-resolution/p6c_bare`
#[test]
fn wf3_three_level_requires_chain_single_method_no_false_ambiguity() {
    let files = &[
        (
            "main.baml",
            r#"function main() -> string { let w = c.W {} return w.f() }"#,
        ),
        ("ns_a/a.baml", r#"interface A { function f(self) -> string }"#),
        ("ns_b/b.baml", r#"interface B requires root.a.A {}"#),
        (
            "ns_c/c.baml",
            r#"
            interface C requires root.b.B {}
            class W {
                implements root.c.C {}
                implements root.b.B {}
                implements root.a.A { function f(self) -> string { return "deep" } }
            }
            "#,
        ),
    ];
    assert_no_compile_errors_multi(files);
}

/// wf3 #12 [medium]: a union whose every member implements `Animal` must be
/// assignable to `Animal` (single `Dog` already is). `Dog | Cat` -> `Animal`.
/// `_plan/wf3/subtyping-optional-union-match/union_assign_to_iface.baml`
#[tokio::test]
async fn wf3_union_of_implementors_assignable_to_interface_runtime() {
    let output = baml_test!(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        class Cat {
            implements Animal {
                function speak(self) -> string { return "Meow." }
            }
        }
        function as_animal(x: Dog | Cat) -> Animal {
            return x
        }
        function main() -> string {
            let c: Cat = Cat {}
            let a = as_animal(c)
            return a.speak()
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::String("Meow.".into()));
}

/// wf3 #13 [medium]: calling `.speak()` on `Animal | Swimmer` must be rejected
/// (a `Swimmer` need not be an `Animal`) — but the diagnostic must blame the arm
/// that lacks `speak` (`Swimmer`), NOT falsely claim `Animal` has no `speak`.
/// `_plan/wf3/subtyping-optional-union-match/union_method_on_iface_union.baml`
#[test]
fn wf3_method_on_interface_union_blames_correct_member() {
    let errors = collect_compile_errors(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        interface Swimmer {
            function swim(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
            implements Swimmer {
                function swim(self) -> string { return "splash" }
            }
        }
        function describe(x: Animal | Swimmer) -> string {
            return x.speak()
        }
        function main() -> string {
            let a: Animal = Dog {}
            return describe(a)
        }
        "#,
    );
    assert!(
        !errors.is_empty(),
        "`.speak()` on `Animal | Swimmer` must be rejected"
    );
    assert!(
        !errors.iter().any(|e| e.contains("Animal")
            && e.to_lowercase().contains("no member")
            && e.contains("speak")),
        "diagnostic must not claim `Animal` lacks `speak` — it declares it; \
         should blame `Swimmer`. Got:\n  {}",
        errors.join("\n  ")
    );
}

/// wf3 #15 [medium]: an out-of-body impl for a PRIMITIVE
/// (`implements Debuggable for int`) must be visible to the reflection registry.
/// The program encodes `implements()==true` as +1000; expect 1001 (1 implementor).
/// `_plan/wf3/out-of-body-throws/oob_primitive_refl_split.baml`
#[tokio::test]
async fn wf3_out_of_body_primitive_impl_visible_to_reflection_runtime() {
    let output = baml_test!(
        r#"
        interface Debuggable {
            function debug(self) -> string
        }
        implements Debuggable for int {
            function debug(self) -> string { return "int" }
        }
        function main() -> int {
            let a = reflect.type_of<int>().implements(reflect.type_of<Debuggable>())
            let impls = reflect.type_of<Debuggable>().implementors()
            if a { return 1000 + impls.length() }
            return impls.length()
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(1001));
}

/// wf3 #16 [medium]: `throws A | B` and `throws B | A` must be equivalent in
/// signature matching (field-position union order already is — see
/// `interface_field_union_order_is_exactly_equivalent`).
/// `_plan/wf3/out-of-body-throws/oob_throws_union.baml`
#[test]
fn wf3_throws_union_order_is_equivalent() {
    assert_no_compile_errors(
        r#"
        class A { a: string }
        class B { b: string }
        interface Fallible {
            function run(self) -> string throws A | B
        }
        class Worker {}
        implements Fallible for Worker {
            function run(self) -> string throws B | A {
                throw A { a: "x" }
            }
        }
        function main() -> string { return "ok" }
        "#,
    );
}

/// wf3 #17 [medium]: `throws`/return signature matching must resolve type
/// aliases — `throws Err` (alias of `IoError`) is satisfied by `throws IoError`.
/// `_plan/wf3/out-of-body-throws/oob_throws_alias.baml`
#[test]
fn wf3_throws_alias_is_resolved_in_signature_match() {
    assert_no_compile_errors(
        r#"
        class IoError { message: string }
        type Err = IoError
        interface Fallible {
            function run(self) -> string throws Err
        }
        class Worker {}
        implements Fallible for Worker {
            function run(self) -> string throws IoError {
                throw IoError { message: "x" }
            }
        }
        function main() -> string { return "ok" }
        "#,
    );
}

/// wf3 #18 [medium]: when `Pair<int, int>` collapses both `Getter<L>` and
/// `Getter<R>` to `Getter<int>`, the duplicate must be diagnosed (an explicit
/// duplicate impl is E0114) — not silently resolved to the first block.
/// `_plan/wf3/generics-core/gen_same_typearg_collision.baml`
#[test]
fn wf3_monomorph_collision_assignment_is_diagnosed() {
    let errors = collect_compile_errors(
        r#"
        interface Getter<T> {
            function get(self) -> T
        }
        class Pair<L, R> {
            left: L
            right: R
            implements Getter<L> {
                function get(self) -> L { return self.left }
            }
            implements Getter<R> {
                function get(self) -> R { return self.right }
            }
        }
        function main() -> int {
            let p: Pair<int, int> = Pair { left: 7, right: 99 }
            let g: Getter<int> = p
            return g.get()
        }
        "#,
    );
    assert!(
        !errors.is_empty(),
        "`Pair<int, int>` collapses both `Getter<L>` and `Getter<R>` to \
         `Getter<int>`; the collision must be diagnosed, not silently resolved \
         to the first impl block"
    );
}

/// wf3 #20 [low]: the E0121 suggestion for a monomorphized generic collision
/// must use the concrete instantiation (`as<Getter<int>>`), not the
/// uninstantiated `as<Getter<L>>` (which fails E0002 `unresolved type: L`).
/// `_plan/wf3/generics-core/gen_mono_collision_unqualified.baml`
#[test]
fn wf3_monomorph_collision_unqualified_suggestion_uses_concrete_args() {
    assert_compile_error_contains(
        r#"
        interface Getter<T> {
            function get(self) -> T
        }
        class Pair<L, R> {
            left: L
            right: R
            implements Getter<L> {
                function get(self) -> L { return self.left }
            }
            implements Getter<R> {
                function get(self) -> R { return self.right }
            }
        }
        function main() -> int {
            let p: Pair<int, int> = Pair { left: 7, right: 99 }
            return p.get()
        }
        "#,
        "Getter<int>",
    );
}

// ── Low-severity / diagnostic quality (desired direction) ────────────────────

/// wf3 #19 [low]: a `requires <unknown>` E0112 must echo the user-written
/// qualifier (`root.a.Ghost`), not strip it to the bare leaf `Ghost` — note that
/// `implements` already preserves the qualifier.
/// `_plan/wf3/namespace-resolution/p3b_root_requires`
#[test]
fn wf3_bare_cross_ns_requires_unknown_echoes_qualifier() {
    let files = &[
        ("main.baml", r#"function main() -> string { return "ok" }"#),
        (
            "ns_a/a.baml",
            r#"interface Base { function f(self) -> string }"#,
        ),
        ("ns_b/b.baml", r#"interface Derived requires root.a.Ghost {}"#),
    ];
    assert_compile_error_contains_multi(files, "root.a.Ghost");
}

/// wf3 [medium]: an unsatisfied blanket bound must name the failed bound — e.g.
/// `Box<Rock>` against `implements<T extends Named> Printable for Box<T>` should
/// mention `Named`, not a bare `type mismatch: expected Printable, got Box<Rock>`.
/// `_plan/wf3/generics-bounds-blanket/p12_bound_fail_msg.baml`
#[test]
fn wf3_unsatisfied_blanket_bound_message_names_bound() {
    assert_compile_error_contains(
        r#"
        interface Named { name: string }
        interface Printable { function display(self) -> string }
        class Rock { label: string }
        class Box<T> { value: T }
        implements<T extends Named> Printable for Box<T> {
            function display(self) -> string { return self.value.name }
        }
        function main() -> string {
            let item: Printable = Box<Rock> { value: Rock { label: "ig" } }
            return item.display()
        }
        "#,
        "Named",
    );
}

/// wf3 [low]: `.as<I>` to an interface the receiver does not implement should
/// say so (mention "implement"), not a generic `type mismatch: expected I, got C`.
/// `_plan/wf3/dispatch-as-projection/as_to_unimplemented.baml`
#[test]
fn wf3_as_to_unimplemented_interface_message_mentions_implement() {
    assert_compile_error_contains(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        interface Vehicle {
            function drive(self) -> string
        }
        class Cat {
            implements Animal {
                function speak(self) -> string { return "Meow." }
            }
        }
        function main() -> string {
            let c = Cat {}
            return c.as<Vehicle>.drive()
        }
        "#,
        "implement",
    );
}

/// wf3 [low/design]: `let x = default` (using `default` as a bare value) should
/// be a compile error — `default` is only meaningful in call position
/// (`default.method(...)`). `_plan/wf3/default-methods/p5_default_bare.baml`
#[test]
fn wf3_default_as_bare_value_is_rejected() {
    assert_compile_error_contains(
        r#"
        interface Logger {
            function log(self, msg: string) -> string { return msg }
        }
        class C {
            implements Logger {
                function log(self, msg: string) -> string {
                    let x = default
                    return msg
                }
            }
        }
        function main() -> string {
            let c = C {}
            return c.log("hi")
        }
        "#,
        "default",
    );
}

/// wf3 [low/limitation]: inside an interface's own `implements` block, an
/// *aliased* interface field (`name as title`) accessed by its interface name
/// (`self.name`) requires the explicit `self.as<Named>.name` projection — a bare
/// `self.name` is a clean E0007 directing to it. (Resolving the bare form would
/// need MIR field-link lowering of `self.<view>`; the projection form is the
/// supported spelling, and `self.title` — the real class field — also works.)
/// `_plan/wf3/field-views/self_field_access_in_method.baml`
#[test]
fn wf3_self_interface_field_access_in_method_requires_projection() {
    // The bare `self.name` (aliased view) is rejected with a projection hint.
    assert_compile_error_contains(
        r#"
        interface Named {
            name: string
            function greet(self) -> string
        }
        class Person {
            title: string
            implements Named {
                name as title
                function greet(self) -> string {
                    return "Hi " + self.name
                }
            }
        }
        "#,
        "as<Named>",
    );
}

/// Companion to the above: the supported `self.as<Named>.name` projection
/// resolves and runs.
#[tokio::test]
async fn wf3_self_interface_field_access_via_projection_runtime() {
    let output = baml_test!(
        r#"
        interface Named {
            name: string
            function greet(self) -> string
        }
        class Person {
            title: string
            implements Named {
                name as title
                function greet(self) -> string {
                    return "Hi " + self.as<Named>.name
                }
            }
        }
        function main() -> string {
            let p = Person { title: "Ada" }
            let n: Named = p
            return n.greet()
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::String("Hi Ada".into()));
}

/// wf3 [low/design]: a `throws` narrower than the interface's declaration
/// (`throws NetworkError` where `NetworkError implements IError` and the
/// interface declares `throws IError`) should be allowed by covariance — today
/// it is rejected E0120 even though throwing a subtype at a throw-site is fine.
/// `_plan/wf3/out-of-body-throws/oob_throws_subset.baml`
#[test]
fn wf3_throws_covariant_narrower_is_allowed() {
    assert_no_compile_errors(
        r#"
        interface IError {
            function describe(self) -> string
        }
        class NetworkError {
            msg: string
            implements IError {
                function describe(self) -> string { return "net" }
            }
        }
        interface Fallible {
            function run(self) -> string throws IError
        }
        class Worker {}
        implements Fallible for Worker {
            function run(self) -> string throws NetworkError {
                throw NetworkError { msg: "x" }
            }
        }
        function main() -> string { return "ok" }
        "#,
    );
}

/// wf3 [design decision O2]: `implements B {}` where `B requires A` requires an
/// explicit `implements A {}` even when `A` is all-default and field-less. The
/// BEP contradicts itself here (§772 requires named impls; §1381 says all-default
/// ⇒ valid); we keep E0125 STRICT — every `requires` parent must be named — so a
/// class's contract is always explicit at its declaration. (Decided with the
/// user; the relaxation was the alternative.)
/// `_plan/wf3/requires-diamond/p9_implicit_parent_all_defaults.baml`
#[test]
fn wf3_all_default_requires_parent_still_needs_explicit_implements() {
    assert_compile_error_code(
        r#"
        interface A {
            function greet(self) -> string { return "hi" }
        }
        interface B requires A {
            function bye(self) -> string { return "bye" }
        }
        class C {
            implements B {}
        }
        function main() -> string {
            let c: B = C {}
            return c.bye()
        }
        "#,
        "E0125",
    );
}

/// wf3 [low]: a `requires` cycle diagnostic should report the full path
/// (`A -> B -> A`), not one node per error.
/// `_plan/wf3/requires-diamond/p7_requires_cycle.baml`
#[test]
fn wf3_requires_cycle_reports_full_path() {
    assert_compile_error_contains(
        r#"
        interface A requires B { function fa(self) -> string }
        interface B requires A { function fb(self) -> string }
        class C {
            implements A { function fa(self) -> string { return "a" } }
            implements B { function fb(self) -> string { return "b" } }
        }
        function main() -> string {
            let c: A = C {}
            return c.fa()
        }
        "#,
        "A -> B",
    );
}

/// wf3 [low]: out-of-body / blanket method must be callable directly on an
/// instance (`x.debug()`), not only through an interface binding. In-body impls
/// already allow the direct call.
/// `_plan/wf3/generics-bounds-blanket/p16_outofbody_concrete_direct.baml`
#[tokio::test]
async fn wf3_out_of_body_method_callable_directly_runtime() {
    let output = baml_test!(
        r#"
        interface Debuggable { function debug(self) -> string }
        implements Debuggable for int {
            function debug(self) -> string { return "int" }
        }
        function main() -> string {
            let x: int = 5
            return x.debug()
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::String("int".into()));
}

/// wf3 [low]: a blanket method on a generic class must be callable directly on
/// an instance (`b.display()`).
/// `_plan/wf3/generics-bounds-blanket/p15_direct_call_no_annotation.baml`
#[tokio::test]
async fn wf3_blanket_method_callable_directly_runtime() {
    let output = baml_test!(
        r#"
        interface Printable { function display(self) -> string }
        class Box<T> { value: T }
        implements<T> Printable for Box<T> {
            function display(self) -> string { return "box" }
        }
        function main() -> string {
            let b = Box<int> { value: 42 }
            return b.display()
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::String("box".into()));
}

// ── Correct-today behavior pinned against regression (works-confirmations) ───

/// wf3 pin: out-of-body impl for a field-bearing interface on a primitive is
/// correctly rejected (E0126). `_plan/wf3/out-of-body-throws/oob_primitive_field_diag.baml`
#[test]
fn wf3_out_of_body_primitive_field_bearing_is_e0126_pins() {
    assert_compile_error_code(
        r#"
        interface Named {
            name: string
            function display(self) -> string
        }
        implements Named for int {
            function display(self) -> string { return "int" }
        }
        function main() -> string { return "ok" }
        "#,
        "E0126",
    );
}

/// wf3 pin: when a value matches two blanket rules (`int[]?` satisfies both
/// `for T[]` and `for T?`), the innermost structural (list) match wins. Pins the
/// (currently undocumented) precedence so a change is noticed.
/// `_plan/wf3/generics-bounds-blanket/p2_optional_list_ambiguous.baml`
#[tokio::test]
async fn wf3_two_blanket_rules_list_wins_pins() {
    let output = baml_test!(
        r#"
        interface Label {
            function label(self) -> string
        }
        implements<T> Label for T[] {
            function label(self) -> string { return "list" }
        }
        implements<T> Label for T? {
            function label(self) -> string { return "optional" }
        }
        function main() -> string {
            let xs: int[]? = [1, 2, 3]
            let labelled: Label = xs
            return labelled.label()
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::String("list".into()));
}

/// wf3 pin: a method present on every member of a concrete-class union is
/// callable directly on the union (`(Dog | Cat).speak()`). Pinned to highlight
/// the inconsistency with `wf3_union_of_implementors_assignable_to_interface`
/// (#12): you can call the method but cannot up-cast the union to the interface.
/// `_plan/wf3/subtyping-optional-union-match/union_dog_cat_method_in_arms.baml`
#[tokio::test]
async fn wf3_union_member_method_call_works_pins() {
    let output = baml_test!(
        r#"
        interface Animal {
            function speak(self) -> string
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        class Cat {
            implements Animal {
                function speak(self) -> string { return "Meow." }
            }
        }
        function describe(x: Dog | Cat) -> string {
            return x.speak()
        }
        function main() -> string {
            let c: Cat = Cat {}
            return describe(c)
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::String("Meow.".into()));
}

/// wf3 pin: a class's own concrete field shadows same-named interface-field
/// views — unqualified `i.name` reads the concrete field. Correct per the Group D
/// shadow rule; pinned because the outcome silently flips based on whether a
/// concrete field exists. `_plan/wf3/field-views/ambig_mixed_autolink_alias.baml`
#[tokio::test]
async fn wf3_concrete_field_shadows_interface_views_pins() {
    let output = baml_test!(
        r#"
        interface Named { name: string }
        interface Labeled { name: string }
        class Item {
            name: string
            other: string
            implements Named {}
            implements Labeled { name as other }
        }
        function main() -> string {
            let i = Item { name: "N", other: "O" }
            return i.name
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::String("N".into()));
}

/// wf3 pin: a bare generic interface in reflection (`reflect.type_of<Box>()`,
/// no type args) currently acts as an undocumented wildcard matching every
/// instantiation's implementors. Pinned to flag the behavior.
/// `_plan/wf3/generics-reflection/gen_bare_impl_both.baml`
#[tokio::test]
async fn wf3_bare_generic_interface_reflection_is_wildcard_pins() {
    let output = baml_test!(
        r#"
        interface Box<T> {
            function get(self) -> T
        }
        class IntBox {
            implements Box<int> {
                function get(self) -> int { return 1 }
            }
        }
        class StringBox {
            implements Box<string> {
                function get(self) -> string { return "hi" }
            }
        }
        function main() -> bool {
            let bare = reflect.type_of<Box>()
            return bare.implemented_by(reflect.type_of<IntBox>())
                && bare.implemented_by(reflect.type_of<StringBox>())
                && bare.implementors().length() == 2
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

/// wf3 pin: a blanket impl's implementor is reported by reflection as the bare
/// generic class name (`Box`), not a concrete `Box<int>`.
/// `_plan/wf3/generics-reflection/gen_blanket_id.baml`
#[tokio::test]
async fn wf3_blanket_implementor_identity_is_bare_class_pins() {
    let output = baml_test!(
        r#"
        interface Printable {
            function display(self) -> string
        }
        class Box<T> {
            value: T
        }
        implements<T> Printable for Box<T> {
            function display(self) -> string { return "box" }
        }
        function main() -> string {
            let p = reflect.type_of<Printable>()
            return p.implementors()[0].to_string()
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::String("Box".into()));
}
