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
use baml_project::{collect_diagnostics, testing::setup_test_db};
use baml_tests::baml_test;
use baml_type::Ty;
use bex_engine::BexExternalValue;

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Collect compile errors raised in user files. Returns `[E0123] message`
/// strings so tests can assert against the public diagnostic code without
/// being tied to exact wording.
fn collect_compile_errors(source: &str) -> Vec<String> {
    let db = setup_test_db(source);
    let project = db.get_project().expect("project must be set");
    let all_files = db.get_source_files();
    let user_file_ids: HashSet<_> = all_files.iter().map(|f| f.file_id(&db)).collect();

    collect_diagnostics(&db, project, &all_files)
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
fn assert_no_interface_errors(source: &str) {
    let errors = collect_compile_errors(source);
    // Interface errors all live in the E0112-E0131 range.
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
        "required method `speak` of interface `Animal`",
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
        "required method `introduce` of interface `Person`",
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
    // Inside `d: Dog`, the binding `d` should be typed as `Dog`, letting
    // class-specific fields like `breed` be accessed.
    assert_no_interface_errors(
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
                d: Dog => d.breed
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

#[test]
fn interface_default_method_requires_receiver_projection() {
    assert_compile_error_contains(
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
            return Describable.describe(t)
        }
        "#,
        "must be accessed through a value",
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
async fn reflect_implementors_lists_declaration_order_and_identity() {
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
                && impls[0] == reflect.type_of<Dog>()
                && impls[1] == reflect.type_of<Cat>()
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
        "target must be an interface",
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
fn generic_interface_default_method_requires_receiver_projection() {
    assert_compile_error_contains(
        r#"
        interface Label<T> {
            function label(self) -> string {
                return "ok"
            }
        }
        class Box {
            implements Label<int> {}
        }
        function main() -> string {
            return Label<int>.label(Box {})
        }
        "#,
        "must be accessed through a value",
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
