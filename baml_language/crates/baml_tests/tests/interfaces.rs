//! Tests for the `interface` declaration and `implements I { ... }` blocks
//! introduced by BEP-044.
//!
//! These tests exercise the full pipeline: parser + AST lowering + per-file
//! interface validation, plus method dispatch, default-body resolution,
//! `.as<I>` projections, generic monomorphisation, and reflection executed
//! end-to-end through the BAML VM (see BEP-044 §"Method Disambiguation").
//!
//! The shape of each test is one of:
//!   1. Compile-time: compile a self-contained BAML snippet through the
//!      project pipeline, collect compile errors that originate in the user
//!      file, and assert presence (or absence) of specific diagnostic codes /
//!      messages. This is the bulk of the file.
//!   2. Runtime: execute the program through the VM. Only a handful remain —
//!      the runtime behavior of interfaces now lives in
//!      `baml_src/ns_interfaces/` as native `test` blocks, which compile once
//!      for the whole corpus instead of once per test. Do NOT add a runtime
//!      test here; see `baml_language/TEST_INSTRUCTIONS.md`. The survivors
//!      each need something BAML cannot observe:
//!        - `aliased_interface_fields_do_not_create_concrete_runtime_slots`
//!          and `interface_return_uses_concrete_implementor_field_shape`
//!          assert the host-marshalled field map, i.e. that an aliased
//!          interface field has no concrete runtime slot at all;
//!        - the two `fuzz_bug*` tests are `#[ignore]`d records of unsupported
//!          behavior, and BAML `test` blocks have no `ignore`.
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
use baml_project::ProjectDatabase;
use baml_tests::{
    baml_test,
    stdlib_prefix::{check_user_files, setup_multi_file_db, setup_test_db},
};
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
    let db = setup_multi_file_db(files);
    collect_compile_errors_from_db(&db)
}

fn collect_compile_errors_from_db(db: &ProjectDatabase) -> Vec<String> {
    let all_files = db.get_source_files();
    let user_file_ids: HashSet<_> = all_files.iter().map(|f| f.file_id(db)).collect();

    check_user_files(db)
        .into_iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .filter(|d| {
            d.primary_span()
                .map(|span| user_file_ids.contains(&span.file_id))
                .unwrap_or(false)
        })
        .map(|d| format!("[{}] {}", d.code(), d.message_with_primary_label()))
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
            function speak(self) -> string throws never
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
            function display(self) -> string throws never {
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
            function introduce(self) -> string throws never
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
            function size(self) -> int throws never {
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
            function speak(self) -> string throws never
        }
        interface Swimmer {
            function swim(self) -> string throws never
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
            function speak(self) -> string throws never
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
            function speak(self) -> string throws never
        }
        class Mute {
            implements Animal {}
        }
        "#,
        "method `speak` required by interface `Animal`",
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
        // Unknown interface in `implements` surfaces as the general unresolved-type
        // error (E0002 `unresolved type: DoesNotExist`), not a dedicated E0112.
        "E0002",
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
fn orphan_rule_rejects_blanket_impl_of_foreign_interface() {
    // BEP-044 orphan rule (E0139): a blanket impl of a *foreign* interface
    // (`baml.ops.Equals`) over a bare type parameter is the classic violation (the
    // "smuggling" shape) — `T` is uncovered and no local type anchors the impl.
    assert_compile_error_code(
        r#"
        implement<T> baml.ops.Equals for T {}
        "#,
        "E0139",
    );
}

#[test]
fn orphan_rule_rejects_foreign_interface_for_foreign_type() {
    // Foreign interface + foreign type (`int`): neither is local to this package.
    assert_compile_error_code(
        r#"
        implement baml.ops.Equals for int {}
        "#,
        "E0139",
    );
}

#[test]
fn orphan_rule_allows_local_interface_blanket() {
    // Implementing your *own* interface is always allowed, even as a blanket over
    // a bare type parameter — the interface being local satisfies the orphan rule.
    assert_no_compile_errors(
        r#"
        interface Marker {}
        implement<T> Marker for T {}
        "#,
    );
}

#[test]
fn unknown_method_in_implements_block_is_compile_error() {
    assert_compile_error_code(
        r#"
        interface Animal {
            function speak(self) -> string throws never
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
            function speak(self) -> string throws never
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
        "E0132",
    );
}

#[test]
fn duplicate_same_generic_interface_instantiation_is_compile_error() {
    assert_compile_error_code(
        r#"
        interface Converter<T> {
            function convert(self) -> T throws never
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
        "E0132",
    );
}

#[test]
fn duplicate_generic_interface_instantiation_via_alias_is_compile_error() {
    // `Converter<IntAlias>` (where `type IntAlias = int`) and `Converter<int>`
    // are the same realized view, so implementing both is a duplicate even though
    // the source text differs — duplicate detection keys on the lowered args.
    assert_compile_error_code(
        r#"
        interface Converter<T> {
            function convert(self) -> T throws never
        }

        type IntAlias = int

        class MultiFormat {
            implements Converter<IntAlias> {
                function convert(self) -> int { return 1 }
            }
            implements Converter<int> {
                function convert(self) -> int { return 2 }
            }
        }
        "#,
        "E0132",
    );
}

#[test]
fn nested_alias_duplicate_implements_is_detected() {
    // The alias is nested *inside* a list rather than at the top of the type
    // argument: `Converter<IntList[]>` (where `type IntList = int[]`) realizes to
    // `Converter<int[][]>`, so the two `implements` are duplicates. Detecting this
    // requires expanding the alias at every nesting depth when building the dedup
    // key — expanding only a top-level alias would leave these keys distinct and
    // miss the duplicate.
    assert_compile_error_code(
        r#"
        interface Converter<T> {
            function convert(self) -> T throws never
        }

        type IntList = int[]

        class MultiFormat {
            implements Converter<IntList[]> {
                function convert(self) -> int[][] { return [[1]] }
            }
            implements Converter<int[][]> {
                function convert(self) -> int[][] { return [[2]] }
            }
        }
        "#,
        "E0132",
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
            function introduce(self) -> string throws never
        }

        class Employee {
            salary: float
            implements Person {}
        }
        "#,
        "method `introduce` required by interface `Person`",
    );
}

#[test]
fn requires_chain_required_method_must_be_provided() {
    // `Person` requires `Greeter`. Implementing `Person` without an explicit
    // `implements Greeter` is rejected: the required parent must be implemented
    // (E0125), which is what carries `Greeter`'s `greet` obligation.
    assert_compile_error_contains(
        r#"
        interface Greeter {
            function greet(self) -> string throws never
        }
        interface Person requires Greeter {
            name: string
        }

        class Bob {
            implements Person {}
        }
        "#,
        "also requires implementing `Greeter`",
    );
}

// ── Group F: misc / regression ──────────────────────────────────────────────

#[test]
fn empty_implements_block_with_all_defaults_is_ok() {
    assert_no_interface_errors(
        r#"
        interface Printable {
            function display(self) -> string throws never { return "x" }
            function verbose(self) -> string throws never { return "y" }
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
            function close(self) -> null throws never
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
            function speak(self) -> string throws never
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
    // Regression: interface-specific diagnostics we emit stay in the E0112+
    // range we reserved in `baml_compiler_diagnostics`. (Unknown target names
    // are a general name-resolution failure and surface as E0002 instead.)
    let bad_cases: &[(&str, &str)] = &[
        // (snippet, expected code)
        (
            "interface I { function f(self) -> string } class C { implements I {} }",
            "E0113",
        ),
        // Unknown interface names surface as the general E0002 unresolved-type
        // error rather than a dedicated interface-range code.
        ("class C { implements Missing {} }", "E0002"),
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
            function speak(self) -> string throws never
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
            function speak(self) -> string throws never
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
            function speak(self) -> string throws never
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
            function add(self, a: int, b: int) -> int throws never
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
fn impl_may_narrow_interface_method_throws() {
    // Throws is covariant in method conformance: an impl that throws *less* than the
    // interface method declares still conforms. Here the interface's `run` declares
    // `throws IoError` but the impl's `run` is infallible (`throws never`), which is a
    // subtype — so this is accepted, not an E0120 signature mismatch.
    assert_zero_compile_errors(
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
    );
}

#[test]
fn method_signature_match_is_ok() {
    assert_no_interface_errors(
        r#"
        interface Adder {
            function add(self, a: int, b: int) -> int throws never
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
            function label(self, value: bool) -> string throws never {
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
    // A function parameter typed as the interface exposes the interface's
    // methods on the value, even though the value is a concrete class.
    //
    // This test pins that interface-typed parameters accept concrete
    // instances and that fields declared on the interface are visible
    // through the interface-typed variable.
    assert_no_interface_errors(
        r#"
        interface Animal {
            name: string
            function speak(self) -> string throws never
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
            function encode(self) -> string throws never
        }
        interface BinarySerializer {
            function encode(self) -> string throws never
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
            function encode(self) -> string throws never
        }
        interface BinarySerializer {
            function encode(self) -> string throws never
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
            function id(self) -> string throws never
        }
        interface B {
            function id(self) -> string throws never
        }
        interface C {
            function id(self) -> string throws never
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
            function encode(self) -> string throws never
        }
        interface BinarySerializer {
            function encode(self) -> string throws never
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
            function speak(self) -> string throws never
        }
        interface Swimmer {
            function swim(self) -> string throws never
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
            function speak(self) -> string throws never
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
            function speak(self) -> string throws never
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
            function speak(self) -> string throws never
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
            function speak(self) -> string throws never
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }

        function main() -> bool {
            let dog_t = reflect.Type.of<Dog>()
            let animal_t = reflect.Type.of<Animal>()
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
            function speak(self) -> string throws never
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }

        function main() -> bool {
            let dog_t = reflect.Type.of<Dog>()
            let animal_t = reflect.Type.of<Animal>()
            return animal_t.implemented_by(dog_t)
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
            function speak(self) -> string throws never
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
            function introduce(self) -> string throws never
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

    let Ok(BexExternalValue::Instance {
        class_name, fields, ..
    }) = output.result
    else {
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

    let Ok(BexExternalValue::Instance {
        class_name, fields, ..
    }) = output.result
    else {
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

// `Slot<L, R>` and `Slot<R, L>` on `Pair<L, R>` realize the same interface
// `Slot<T, T>` at the diagonal `Pair<T, T>`, so they overlap and are rejected.
#[test]
fn generic_interface_field_links_swapped_type_var_impls_overlap() {
    assert_compile_error_code(
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
    "#,
        "E0132",
    );
}

// As above, but the interface defines a method rather than a field link.
#[test]
fn generic_interface_method_swapped_type_var_impls_overlap() {
    assert_compile_error_code(
        r#"
        interface Reporter<T, E> {
            function show(self) -> T throws never
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
    "#,
        "E0132",
    );
}

// ── Group M: default.method() ───────────────────────────────────────────────

// ── Group N: `.as<I>` projections ───────────────────────────────────────────

#[test]
fn old_interface_qualified_projection_is_compile_error() {
    assert_compile_error_contains(
        r#"
        interface Animal {
            function speak(self) -> string throws never
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
        // The old `d.Interface.method()` syntax is not special-cased: `Animal`
        // is simply not a member of `Dog`, so this is a plain no-member error.
        "has no member `Animal`",
    );
}

// ── Group O: dispatch through interface-typed value ─────────────────────────

#[test]
fn self_param_method_rejects_heterogeneous_generic_args() {
    // Soundness: `Self` is rigid per receiver. `x: S` pins `Self = S`, so the
    // `other: Self` argument must also be `S`. Passing `y: U` (an unrelated
    // generic param) must NOT unify `S := U` — it is a type error.
    assert_compile_error_contains(
        r#"
        interface Equatable {
            function eq(self, other: Self) -> bool throws never
        }
        function cmp<S extends Equatable, U extends Equatable>(x: S, y: U) -> bool {
            return x.eq(y)
        }
        "#,
        "expected `S`, found `U`",
    );
}

#[test]
fn self_param_method_rejects_mismatched_literal_arg() {
    // `x: T` pins `Self = T`; the rigid `Self` must not be inferred to `int`
    // from the `5` argument. Passing a concrete literal where `Self` is rigid
    // is a type error.
    assert_compile_error_contains(
        r#"
        interface Equatable {
            function eq(self, other: Self) -> bool throws never
        }
        function bad<T extends Equatable>(x: T) -> bool {
            return x.eq(5)
        }
        "#,
        "expected `T`, found `5`",
    );
}

#[test]
fn generic_class_unannotated_self_is_parameterized() {
    // An unannotated `self` in a generic class must be typed `Wrap<T>`, not bare
    // `Wrap`, so it satisfies a parameterized expected type. Regression for the
    // ParseCache builtin failure: the auto-derived `to_json` passed a bare
    // `self` to `baml.json.to_string<ParseCache<TStream, TFinal>>`. Because the
    // callee's generic is differently named, the class params stay rigid and the
    // argument is *checked* (not deferred), which surfaced the bare `self`.
    assert_zero_compile_errors(
        r#"
        function consume<X>(v: X) -> int { return 1 }
        class Wrap<T> {
            value: T
            function use_self(self) -> int { return consume<Wrap<T>>(self) }
        }
        "#,
    );
}

#[test]
fn self_param_method_rejects_nested_self_mismatch() {
    // Rigid `Self` must be enforced even when it appears *nested* in the
    // parameter type (`Self[]`). `x: T` pins `Self = T`, so `others: Self[]`
    // requires a `T[]`; passing a `U[]` (unrelated generic param) must error —
    // not silently skip validation just because the type still contains a
    // variable.
    assert_compile_error_contains(
        r#"
        interface Adder {
            function addAll(self, others: Self[]) -> int throws never
        }
        function cross<T extends Adder, U extends Adder>(x: T, ys: U[]) -> int {
            return x.addAll(ys)
        }
        "#,
        "expected `T[]`, found `U[]`",
    );
}

#[test]
fn bound_self_method_value_rejects_heterogeneous_arg() {
    // Binding a `Self`-param method to a value (`let f = x.addOne`) keeps `Self`
    // pinned to the receiver's `T`. Calling that value with an unrelated `U` must
    // still be rejected — the pin travels with the (non-generic) function value,
    // so the indirect call is checked just like the direct `x.addOne(y)` form.
    assert_compile_error_contains(
        r#"
        interface Adder {
            function addOne(self, other: Self) -> int throws never
        }
        function cross<T extends Adder, U extends Adder>(x: T, y: U) -> int {
            let f = x.addOne
            return f(y)
        }
        "#,
        "expected `T`, found `U`",
    );
}

// ── Group P: match narrowing ────────────────────────────────────────────────

#[test]
fn mixed_interface_and_concrete_destructures_do_not_require_wildcard() {
    assert_zero_compile_errors(
        r#"
        interface Animal {
            name: string
        }
        class Dog {
            name: string
            breed: string
            implements Animal {}
        }

        function describe(a: Animal) -> string {
            return match (a) {
                Dog { breed: "Lab" } => "lab"
                Animal { name: string } => "animal"
            }
        }
        "#,
    );
}

#[test]
fn interface_destructure_before_concrete_destructure_is_unreachable_not_nonexhaustive() {
    let errors = collect_compile_errors(
        r#"
        interface Animal {
            name: string
        }
        class Dog {
            name: string
            breed: string
            implements Animal {}
        }

        function describe(a: Animal) -> string {
            return match (a) {
                Animal { name: string } => "animal"
                Dog { breed: "Lab" } => "lab"
            }
        }
        "#,
    );
    assert!(
        errors.iter().any(|e| e.starts_with("[E0063]")),
        "expected unreachable-arm error, got:\n  {}",
        errors.join("\n  ")
    );
    assert!(
        !errors.iter().any(|e| e.starts_with("[E0062]")),
        "expected no non-exhaustive error, got:\n  {}",
        errors.join("\n  ")
    );
}

#[test]
fn concrete_then_interface_destructure_covers_remaining_implementor_values() {
    assert_zero_compile_errors(
        r#"
        interface Animal {
            friendly: bool
        }
        class Dog {
            friendly: bool
            breed: string
            implements Animal {}
        }

        function describe(a: Animal) -> string {
            return match (a) {
                Dog { friendly: true } => "friendly dog"
                Animal { friendly: true } => "other friendly animal"
                Animal { friendly: false } => "other animal"
            }
        }
        "#,
    );
}

#[test]
fn mixed_interface_and_concrete_destructures_project_aliased_fields() {
    assert_zero_compile_errors(
        r#"
        interface Toggle {
            on: bool
        }
        class Widget {
            label: string
            enabled: bool
            implements Toggle {
                on as enabled
            }
        }

        function describe(t: Toggle) -> string {
            return match (t) {
                Widget { enabled: true } => "on"
                Toggle { on: true } => "other on"
                Toggle { on: false } => "off"
            }
        }
        "#,
    );
}

#[test]
fn mixed_interface_and_generic_concrete_destructures_project_class_type_args() {
    assert_zero_compile_errors(
        r#"
        interface Slot<T> {
            value: T
        }
        class Pair<L, R> {
            left: L
            right: R
            implements Slot<L> {
                value as left
            }
        }

        function describe(s: Slot<int>) -> string {
            return match (s) {
                Pair<int, string> { right: "seven" } => "pair"
                Slot<int> { value: int } => "slot"
            }
        }
        "#,
    );
}

#[test]
fn mixed_interface_and_generic_concrete_destructures_project_swapped_class_type_args() {
    assert_zero_compile_errors(
        r#"
        interface Cell<T> {
            item: T
        }
        class Pair<L, R> {
            left: L
            right: R
            implements Cell<R> {
                item as right
            }
        }

        function describe(c: Cell<string>) -> string {
            return match (c) {
                Pair<int, string> { left: 7 } => "pair"
                Cell<string> { item: string } => "cell"
            }
        }
        "#,
    );
}

#[test]
// `Slot<L>` and `Slot<R>` on `Pair<L, R>` realize the same interface `Slot<T>`
// at the diagonal `Pair<T, T>`, so they overlap and are rejected.
fn mixed_interface_generic_overlapping_type_arg_impls_overlap() {
    assert_compile_error_code(
        r#"
        interface Slot<T> {
            value: T
        }
        class Pair<L, R> {
            left: L
            right: R
            implements Slot<L> {
                value as left
            }
            implements Slot<R> {
                value as right
            }
        }

        function describe(s: Slot<string>) -> string {
            return match (s) {
                Pair<int, string> { left: 7 } => "pair"
                Slot<string> { value: string } => "slot"
            }
        }
        "#,
        "E0132",
    );
}

#[test]
fn generic_interface_and_concrete_destructures_still_report_partial_coverage() {
    assert_compile_error_code(
        r#"
        interface Slot<T> {
            value: T
        }
        class Pair<L, R> {
            left: L
            right: R
            implements Slot<L> {
                value as left
            }
        }

        function describe(s: Slot<int>) -> string {
            return match (s) {
                Pair<int, string> { right: "seven" } => "pair"
                Slot<int> { value: 1 } => "one"
            }
        }
        "#,
        "E0062",
    );
}

#[test]
fn union_of_generic_interfaces_allows_mixed_interface_and_concrete_destructures() {
    assert_zero_compile_errors(
        r#"
        interface Slot<T> {
            value: T
        }
        class Pair<L, R> {
            left: L
            right: R
            implements Slot<L> {
                value as left
            }
        }

        interface Cargo<T> {
            payload: T
        }
        class Box<T> {
            inner: T
            label: string
            implements Cargo<T> {
                payload as inner
            }
        }

        function describe(x: Slot<int> | Cargo<string>) -> string {
            return match (x) {
                Pair<int, string> { right: "tag" } => "pair"
                Slot<int> { value: int } => "slot"
                Box<string> { label: "crate" } => "box"
                Cargo<string> { payload: string } => "cargo"
            }
        }
        "#,
    );
}

#[test]
fn union_of_generic_interfaces_reports_missing_interface_branch() {
    assert_compile_error_code(
        r#"
        interface Slot<T> {
            value: T
        }
        class Pair<L, R> {
            left: L
            right: R
            implements Slot<L> {
                value as left
            }
        }

        interface Cargo<T> {
            payload: T
        }
        class Box<T> {
            inner: T
            label: string
            implements Cargo<T> {
                payload as inner
            }
        }

        function describe(x: Slot<int> | Cargo<string>) -> string {
            return match (x) {
                Pair<int, string> { right: "tag" } => "pair"
                Slot<int> { value: int } => "slot"
            }
        }
        "#,
        "E0062",
    );
}

#[test]
fn union_of_interfaces_allows_mixed_interface_and_concrete_destructures() {
    assert_zero_compile_errors(
        r#"
        interface Animal {
            awake: bool
        }
        class Dog {
            awake: bool
            breed: string
            implements Animal {}
        }

        interface Vehicle {
            running: bool
        }
        class Car {
            running: bool
            make: string
            implements Vehicle {}
        }

        function describe(x: Animal | Vehicle) -> string {
            return match (x) {
                Dog { awake: true } => "dog"
                Animal { awake: true } => "animal"
                Animal { awake: false } => "animal"
                Car { running: true } => "car"
                Vehicle { running: true } => "vehicle"
                Vehicle { running: false } => "vehicle"
            }
        }
        "#,
    );
}

#[test]
fn mixed_interface_and_concrete_destructures_still_report_partial_coverage() {
    assert_compile_error_code(
        r#"
        interface Animal {
            name: string
            friendly: bool
        }
        class Dog {
            name: string
            friendly: bool
            implements Animal {}
        }

        function describe(a: Animal) -> string {
            return match (a) {
                Dog { friendly: true } => "friendly"
                Animal { name: "Rex" } => "rex"
            }
        }
        "#,
        "E0062",
    );
}

// ── Group Q: generics ───────────────────────────────────────────────────────

// ── Group R: reflection ─────────────────────────────────────────────────────

// ── Group S: class methods + interface coexist ──────────────────────────────

#[test]
fn class_inherent_method_does_not_satisfy_interface_method() {
    // `Person`'s `greet` is an inherent method (outside the `implements` block), so
    // it does NOT satisfy the abstract `Greeter.greet` (BEP-044: only `implements`-
    // block members satisfy a requirement). The empty block leaves `greet`
    // unimplemented → E0113 (MissingInterfaceMethod). Its mismatched `-> int`
    // signature is irrelevant — the inherent method is unrelated to the impl.
    assert_compile_error_code(
        r#"
        interface Greeter {
            function greet(self) -> string throws never
        }

        class Person {
            implements Greeter {}

            function greet(self) -> int {
                return 1
            }
        }
        "#,
        "E0113",
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
fn generic_bound_violation_in_instantiation_expr_is_compile_error() {
    // BEP-044 bound enforcement must also apply when a generic callable is
    // referenced as a VALUE (`let f = first_name<int>`), not only at call
    // sites. `int` does not satisfy `extends Named`, so this is a type error.
    assert_compile_error_contains(
        r#"
        interface Named { name: string }
        function first_name<T extends Named>(items: T[]) -> string {
            return items[0].name
        }
        function main() -> string {
            let f = first_name<int>;
            return "ok"
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
            function convert(self) -> T throws never
        }
        function read_int<T extends Converter<int> as Ints>(m: T) -> int {
            return m.as<Converter<int>>.convert()
        }
        "#,
        "generic bound aliases are not supported",
    );
}

#[test]
fn same_interface_different_type_args_is_not_assignable() {
    assert_compile_error_contains(
        r#"
        interface Box<T> {
            function get(self) -> T throws never
        }
        function bad(x: Box<int>) -> Box<string> {
            return x
        }
        "#,
        "mismatched types",
    );
}

#[test]
fn generic_interface_method_explicit_type_args_are_checked() {
    assert_compile_error_contains(
        r#"
        interface Echo<T> {
            function echo<U>(self, value: U) -> U throws never
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
        "mismatched types",
    );
}

#[test]
fn class_type_reference_rejects_wrong_generic_arg_count() {
    assert_compile_error_contains(
        r#"
        class Box<T> {
            value: T
        }
        function bad(value: Box<int, string>) -> int {
            return 1
        }
        "#,
        "expects 1 type argument(s), got 2",
    );
}

#[test]
fn class_type_reference_rejects_too_few_explicit_generic_arg_count() {
    assert_compile_error_contains(
        r#"
        class Pair<L, R> {
            left: L
            right: R
        }
        function bad(value: Pair<int>) -> int {
            return 1
        }
        "#,
        "expects 2 type argument(s), got 1",
    );
}

#[test]
fn function_call_rejects_wrong_explicit_generic_arg_count() {
    assert_compile_error_contains(
        r#"
        function id<T>(value: T) -> T {
            return value
        }
        function bad() -> int {
            return id<int, string>(1)
        }
        "#,
        "function `id` expects 1 type argument(s), got 2",
    );
}

#[test]
fn function_call_rejects_too_few_explicit_generic_arg_count() {
    assert_compile_error_contains(
        r#"
        function pair<L, R>(left: L, right: R) -> L {
            return left
        }
        function bad() -> int {
            return pair<int>(1, "nope")
        }
        "#,
        "function `pair` expects 2 type argument(s), got 1",
    );
}

#[test]
fn in_body_implements_rejects_wrong_interface_generic_arg_count() {
    assert_compile_error_contains(
        r#"
        interface Label<T> {
            function label(self) -> T throws never
        }
        class Thing {
            implements Label<int, string> {
                function label(self) -> int {
                    return 1
                }
            }
        }
        "#,
        "expects 1 type argument(s), got 2",
    );
}

#[test]
fn in_body_implements_rejects_too_few_explicit_interface_generic_arg_count() {
    assert_compile_error_contains(
        r#"
        interface PairLabel<L, R> {
            function label(self) -> L throws never
        }
        class Thing {
            implements PairLabel<int> {
                function label(self) -> int {
                    return 1
                }
            }
        }
        "#,
        "expects 2 type argument(s), got 1",
    );
}

#[test]
fn out_of_body_implements_rejects_wrong_interface_generic_arg_count() {
    assert_compile_error_contains(
        r#"
        interface Label<T> {
            function label(self) -> T throws never
        }
        class Thing {}
        implements Label<int, string> for Thing {
            function label(self) -> int {
                return 1
            }
        }
        "#,
        "expects 1 type argument(s), got 2",
    );
}

#[test]
fn out_of_body_implements_rejects_too_few_explicit_interface_generic_arg_count() {
    assert_compile_error_contains(
        r#"
        interface PairLabel<L, R> {
            function label(self) -> L throws never
        }
        class Thing {}
        implements PairLabel<int> for Thing {
            function label(self) -> int {
                return 1
            }
        }
        "#,
        "expects 2 type argument(s), got 1",
    );
}

#[test]
fn out_of_body_implements_rejects_wrong_target_class_generic_arg_count() {
    assert_compile_error_contains(
        r#"
        interface Marker {}
        class Box<T> {
            value: T
        }
        implements Marker for Box<int, string> {}
        "#,
        "expects 1 type argument(s), got 2",
    );
}

#[test]
fn out_of_body_implements_rejects_too_few_explicit_target_class_generic_arg_count() {
    assert_compile_error_contains(
        r#"
        interface Marker {}
        class Pair<L, R> {
            left: L
            right: R
        }
        implements Marker for Pair<int> {}
        "#,
        "expects 2 type argument(s), got 1",
    );
}

#[test]
fn required_interface_method_generic_bound_mismatch_is_error() {
    assert_compile_error_code(
        r#"
        interface Named { name: string }
        interface Reader {
            function read<T>(self, value: T) -> string throws never
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
fn generic_bounds_reject_compound_type_expressions() {
    // Bounds are interfaces only — a union, list, or optional type is not an interface,
    // so none can be a function generic bound.
    assert_compile_error_contains(
        r#"
        function keep_union<T extends int | string>(x: T) -> int {
            return 1
        }
        function main() -> int {
            return 1
        }
        "#,
        "is not an interface",
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
            function speak(self) -> string throws never
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
            client: GPT4o
            prompt: `
                Identify the animal from the description: ${description}.
                ${ctx.output_format()}
            `
        }
        "##,
    );
}

#[test]
fn llm_function_returning_interface_enumerates_implementors_in_schema() {
    // BEP-044 §"LLM Functions": a function declared to return an
    // interface must compile, with the schema-rendering side later
    // expanding the interface into a `oneOf` of its implementors at
    // prompt evaluation time. This test pins the type-check surface.
    assert_no_interface_errors(
        r##"
        client<llm> GPT4o {
            provider openai
            options { model "gpt-4o" }
        }
        interface Animal {
            function speak(self) -> string throws never
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
            client: GPT4o
            prompt: `
                Identify the animal: ${description}.
                ${ctx.output_format()}
            `
        }
        "##,
    );
}

// ── Group Y: Self return / param types (BEP-044 deferred) ───────────────────

#[test]
fn multi_self_method_rejected_on_interface_typed_receiver() {
    assert_compile_error_contains(
        r#"
        interface Equatable {
            function same(self, other: Self) -> bool throws never
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
fn concrete_receiver_self_param_method_rejects_wrong_arg() {
    // `Self` on a concrete receiver resolves to that concrete type, so a
    // mismatched argument is a type error — checked by ordinary subtyping, not
    // silently accepted.
    assert_compile_error_contains(
        r#"
        interface Equals {
            function eq(self, other: Self) -> bool throws never
            function neq(self, other: Self) -> bool throws never {
                return !self.eq(other)
            }
        }
        class Num {
            v: int
            implements Equals {
                function eq(self, other: Self) -> bool { return self.v == other.v }
            }
        }
        class Other { w: int }
        function main() -> bool {
            return Num { v: 1 }.neq(Other { w: 2 })
        }
        "#,
        "expected `Num`, found `Other`",
    );
}

#[test]
fn unbounded_generic_forwarded_to_bounded_call_is_rejected() {
    // Soundness: forwarding an *unbounded* generic `U` into a call requiring
    // `T extends Equatable` must be rejected — even though the offending type is
    // itself a type variable. Ordinary inference skips TypeVar→TypeVar binds, so
    // the bound is checked via the captured correspondence; otherwise `U` (any
    // type) would reach `eq` and trap at runtime.
    assert_compile_error_contains(
        r#"
        interface Equatable {
            function eq(self, other: Self) -> bool throws never
        }
        function same<T extends Equatable>(x: T) -> bool {
            return x.eq(x)
        }
        function forward<U>(x: U) -> bool {
            return same(x)
        }
        "#,
        "expected `Equatable`, found `U`",
    );
}

#[test]
fn unbounded_generic_forwarded_through_container_is_rejected() {
    // The same hole, leaked through container structure (`U[]` → `T[]`).
    assert_compile_error_contains(
        r#"
        interface Equatable {
            function eq(self, other: Self) -> bool throws never
        }
        function firstEq<T extends Equatable>(xs: T[]) -> bool {
            return xs[0].eq(xs[0])
        }
        function forward<U>(xs: U[]) -> bool {
            return firstEq(xs)
        }
        "#,
        "expected `Equatable`, found `U`",
    );
}

#[test]
fn default_method_returning_self_is_allowed() {
    // A default (or required) method may return `Self`: inside the body `Self` is the
    // abstract receiver, so `-> Self { return self }` is sound. The old "default
    // method may not return Self" check was dropped as an over-restrictive
    // inheritance-model artifact.
    assert_zero_compile_errors(
        r#"
        interface Cloneable {
            function clone(self) -> Self throws never {
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
            function speak(self) -> string throws never
        }
        interface Swimmer {
            function swim(self) -> string throws never
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

#[test]
fn as_rejects_interface_downcast() {
    assert_compile_error_contains(
        r#"
        interface Animal {
            function speak(self) -> string throws never
        }
        interface Swimmer {
            function swim(self) -> string throws never
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
            function speak(self) -> string throws never
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
        "expected an interface qualifier",
    );
}

// ── Group AA: dispatch edge cases — non-local receivers ─────────────────────

// ── Group AB: diamond + multi-level requires ────────────────────────────────

// ── Group AC: `default` keyword corner cases ────────────────────────────────

#[test]
fn default_keyword_outside_implements_block_is_compile_error() {
    // `default` only resolves inside an `implements` block body. A free
    // function shouldn't see it as a magic identifier.
    assert_compile_error_contains(
        r#"
        interface Logger {
            function log(self, msg: string) -> string throws never { return msg }
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
            function log(self, msg: string) -> string throws never { return msg }
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

// ── Group AE: dispatch through deeply nested field/array/index chains ───────

// ── Group AF: dispatch combined with control-flow producing interfaces ──────

// ── Group AG: diamond / requires — deeper trees ─────────────────────────────

// ── Group AH: `default` keyword corner cases ────────────────────────────────

#[test]
fn default_keyword_inside_lambda_inside_implements_block() {
    // `default.log(msg)` referenced from inside a lambda nested in the
    // override is a compile error: per BEP-044, the `default` magic
    // identifier does not capture across closure boundaries — a lambda body
    // sees it as an unresolved name. (Bind the default's result outside the
    // lambda first if you need it.)
    assert_compile_error_contains(
        r#"
        interface Logger {
            function log(self, msg: string) -> string throws never { return msg }
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

// ── Group AI: LLM-context interface composition ─────────────────────────────

#[test]
fn llm_function_with_interface_array_return_compiles() {
    // Returning `Animal[]` from an LLM function should also compile —
    // the schema generator must accept interface types in container
    // positions, not just as the bare top-level return.
    assert_no_interface_errors(
        r##"
        interface Animal {
            function speak(self) -> string throws never
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        function detect_zoo(description: string) -> Animal[] {
            client: GPT4o
            prompt: `
                Identify every animal mentioned in ${description}.
                ${ctx.output_format()}
            `
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
            function speak(self) -> string throws never
        }
        class Dog {
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        function detect_or_describe(description: string) -> Animal | string {
            client: GPT4o
            prompt: `
                If ${description} clearly identifies an animal, return one.
                Otherwise, paraphrase the description.
                ${ctx.output_format()}
            `
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
            function speak(self) -> string throws never
        }
        class Dog {
            name: string
            implements Animal {
                function speak(self) -> string { return "Woof!" }
            }
        }
        function describe_animal(a: Animal) -> string {
            client: GPT4o
            prompt: `
                Describe the animal named ${a.name}.
                ${ctx.output_format()}
            `
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
            function speak(self) -> string throws never
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
            function render(self) -> string throws never
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
fn in_body_inherent_method_does_not_implicitly_satisfy_interface() {
    // `Thing.label` is an inherent method (outside the `implements Label` block),
    // so it does NOT satisfy the abstract `Label.label` — even though the signatures
    // match modulo param names. The empty block leaves `label` unimplemented → E0113
    // (BEP-044: only `implements`-block members satisfy a requirement).
    assert_compile_error_code(
        r#"
        interface Label {
            function label(self, name: string) -> string throws never
        }

        class Thing {
            function label(self, value: string) -> string {
                return value
            }

            implements Label {}
        }
        "#,
        "E0113",
    );
}

#[test]
fn out_of_body_inherent_method_does_not_implicitly_satisfy_interface() {
    // Out-of-body analogue: `Thing.label` is inherent (not in the empty
    // `implements Label for Thing {}` block), so it does NOT satisfy `Label.label`
    // → E0113 (BEP-044).
    assert_compile_error_code(
        r#"
        interface Label {
            function label(self, name: string) -> string throws never
        }

        class Thing {
            function label(self, value: string) -> string {
                return value
            }
        }

        implements Label for Thing {}
        "#,
        "E0113",
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
                    function label(self) -> string throws never
                }
                interface Child requires Parent {}
                "#,
        ),
    ];
    assert_compile_error_contains_multi(files, "E0125");
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
fn interface_requires_same_named_fields_of_different_types_is_allowed() {
    // Interfaces are traits, not inheritance: `X.id` and `Y.id` are distinct
    // per-interface obligations (like `<T as X>::id` vs `<T as Y>::id`), each
    // satisfiable independently via field links. So `Z requires X, Y` with
    // conflicting `id` types is NOT a declaration error — the old E0122
    // requires-field-conflict check was dropped as an inheritance-model artifact.
    assert_zero_compile_errors(
        r#"
        interface X {
            id: string
        }
        interface Y {
            id: int
        }
        interface Z requires X, Y {}
        "#,
    );
}

// ── Group AJ: out-of-body implements (`implements I for T`) ─────────────────

#[test]
fn out_of_body_implements_for_class_compiles() {
    assert_zero_compile_errors(
        r#"
        interface ToJson {
            function to_json(self) -> string throws never
        }
        class Dog { breed: string }
        implements ToJson for Dog {
            function to_json(self) -> string { return self.breed }
        }
        "#,
    );
}

#[test]
fn out_of_body_empty_implements_for_generic_class_does_not_satisfy_via_inherent_method() {
    // `implements Printable for Box<int> {}` is empty, and `Box`'s `label` is an
    // inherent class method (not an `implements`-block member), so it does NOT
    // satisfy the abstract `Printable.label` → E0113 (BEP-044).
    let files = &[
        (
            "main.baml",
            r#"
                interface Printable {
                    function label(self) -> string throws never
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
    assert_compile_error_contains_multi(files, "[E0113]");
}

#[test]
fn out_of_body_empty_implements_does_not_satisfy_abstract_via_inherent_method() {
    // `implements Printable for Box {}` is empty; `Box`'s `label` is an inherent
    // method, which does NOT satisfy the abstract `Printable.label` (BEP-044: only
    // `implements`-block members satisfy a requirement) → E0113.
    assert_compile_error_code(
        r#"
        interface Printable {
            function label(self) -> string throws never
        }

        class Box {
            value: string

            function label(self) -> string {
                return self.value
            }
        }

        implements Printable for Box {}
        "#,
        "E0113",
    );
}

#[test]
fn out_of_body_empty_implements_inherent_method_with_wrong_sig_does_not_satisfy() {
    // Even a same-named inherent method with a *mismatched* signature does not
    // satisfy the abstract `Printable.label`: the inherent method is unrelated to
    // the impl, so `label` is simply unimplemented → E0113 (not a signature error).
    assert_compile_error_code(
        r#"
        interface Printable {
            function label(self) -> string throws never
        }

        class Box {
            function label(self) -> int {
                return 1
            }
        }

        implements Printable for Box {}
        "#,
        "E0113",
    );
}

#[test]
fn out_of_body_implements_field_bearing_interface_is_error() {
    assert_compile_error_code(
        r#"
        interface Named {
            name: string
            function greet(self) -> string throws never
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
            function greet(self) -> string throws never
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
fn out_of_body_implements_inherited_field_bearing_interface_requires_parent() {
    // `Child requires Named`, but `Robot` never implements `Named`, so the
    // missing-required-parent check fires first (E0125). (The out-of-body
    // field-bearing rejection E0126 is covered directly by the sibling tests
    // that implement the field-bearing interface itself.)
    assert_compile_error_code(
        r#"
        interface Named {
            name: string
        }
        interface Child requires Named {}
        class Robot { model: string }

        implements Child for Robot {}
        "#,
        "E0125",
    );
}

#[test]
fn out_of_body_and_in_body_for_same_interface_is_error() {
    assert_compile_error_code(
        r#"
        interface ToJson {
            function to_json(self) -> string throws never
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
        "E0132",
    );
}

#[test]
fn out_of_body_implements_for_unknown_target_is_error() {
    let errors = collect_compile_errors(
        r#"
        interface ToJson {
            function to_json(self) -> string throws never
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
            function to_json(self) -> string throws never
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
            function debug(self) -> string throws never
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
            function debug(self) -> string throws never
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
fn out_of_body_implements_for_generic_function_type_is_rejected() {
    // A function *type* cannot declare generic parameters (function values are
    // realized), so it cannot be the target of an out-of-body `implements`.
    assert_compile_error_contains(
        r#"
        interface GenericCallable {}
        implements GenericCallable for <T>(x: int) -> int {}
        "#,
        "generic parameters",
    );
}

#[test]
fn out_of_body_implements_for_primitive_as_projection_compiles() {
    assert_zero_compile_errors(
        r#"
        interface Debuggable {
            function debug(self) -> string throws never
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
        "expected `int`, found",
    );
}

// The success case — a generic interface's default method as a first-class
// value through the qualifier — lives in the corpus
// (baml_src/ns_item_projections, `generic_interface_default_method_reference`)
// where it pins bytecode and asserts behavior. Only the diagnostic stays here.
#[test]
fn selfless_interface_method_is_not_reachable_through_a_value() {
    // A static method is reached through the TYPE, never a value: the value
    // carries nothing the call needs, and reading it as a receiver is what
    // let it be smuggled into the first real parameter.
    for spelling in ["g.build(1)", "let f = g.build"] {
        let source = format!(
            r#"
        interface Buildable {{
            function build(seed: int) -> Self
        }}
        class Gadget {{
            implements Buildable {{
                function build(seed: int) -> Self throws never {{ Gadget {{}} }}
            }}
        }}
        function main() -> void {{
            let g = Gadget {{}}
            {spelling}
        }}
        "#
        );
        assert_compile_error_code(&source, "E0001");
        // The receiver here is CONCRETE, so the member resolves through the
        // impl rather than the interface slot. Pin the declaring interface in
        // the message: reading it off only the symbolic declarer silently
        // degrades this — the common case — to the un-named wording, which no
        // code-only assertion would catch.
        assert_compile_error_contains(&source, "on interface `Buildable`");
    }
}

#[test]
fn selfless_inherent_method_is_not_reachable_through_a_value() {
    // The same rule for a class-INHERENT static, so the two kinds cannot
    // diverge: `Widget.make(..)` is the only spelling.
    for spelling in ["w.make(1)", "let f = w.make"] {
        let source = format!(
            r#"
        class Widget {{
            n: int
            function make(seed: int) -> Widget throws never {{ Widget {{ n: seed }} }}
        }}
        function main() -> void {{
            let w = Widget {{ n: 0 }}
            {spelling}
        }}
        "#
        );
        assert_compile_error_code(&source, "E0001");
        // The twin of the interface case: no interface declares `make`, so the
        // message names the owning TYPE. Pinned so the un-named wording stays
        // reserved for genuinely inherent statics.
        assert_compile_error_contains(&source, "`TypeName.make(...)`");
    }
}

#[test]
fn bare_interface_method_value_requires_inferable_self() {
    // With nothing pinning `Self` — the value is never called and carries no
    // expectation — the reference is rejected (rustc's `let f = Ord::cmp;`
    // E0790 shape) rather than emitted with an unresolved `Self` frame slot.
    assert_compile_error_code(
        r#"
        interface Label<T> {
            function label(self) -> string throws never {
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
        "E0002",
    );
}

#[test]
fn out_of_body_implements_for_primitive_field_bearing_interface_is_error() {
    assert_compile_error_code(
        r#"
        interface Named {
            name: string
            function display(self) -> string throws never
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
            function describe(self) -> string throws never
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
            function speak(self) -> string throws never
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
            function speak(self) -> string throws never
        }
        interface Swimmer {
            function swim(self) -> string throws never
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
            function speak(self) -> string throws never
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
            function display(self) -> string throws never
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
            function display(self) -> string throws never
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
            function display(self) -> string throws never
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
            function debug(self) -> string throws never
        }
        implements Debuggable for int {
            function debug(self) -> string { return "int" }
        }
    "#,
    );
}

#[test]
fn implements_for_union_target_is_rejected() {
    // BEP-044: the `for` target must be a single concrete type. A union has no
    // single implementation body, so it is rejected (E0138).
    assert_compile_error_code(
        r#"
        interface Tag {
            function tag(self) -> int throws never
        }
        implements Tag for int | string {
            function tag(self) -> int { return 0 }
        }
        "#,
        "E0138",
    );
}

#[test]
fn implements_for_interface_target_is_rejected() {
    // Implementing one interface "for" another (an existential) has no concrete
    // implementor — rejected.
    assert_compile_error_code(
        r#"
        interface Tag {
            function tag(self) -> int throws never
        }
        interface Other {
            function other(self) -> int throws never
        }
        implements Tag for Other {
            function tag(self) -> int { return 0 }
        }
        "#,
        "E0138",
    );
}

#[test]
fn implements_for_optional_target_is_rejected() {
    // An optional is `T | null` — a union — so it has no single implementation
    // body and is rejected just like any other union target (E0138).
    assert_compile_error_code(
        r#"
        interface Label {
            function label(self) -> string throws never
        }
        implements<T> Label for T? {
            function label(self) -> string { return "optional" }
        }
        "#,
        "E0138",
    );
}

#[test]
fn impl_generic_bound_must_be_an_interface() {
    // BEP-044: a generic bound (`T extends X`) must be an interface. A class
    // bound resolves to a concrete non-interface type and is rejected (E0145),
    // rather than being silently dropped from the resolved bound set.
    assert_compile_error_code(
        r#"
        interface Printable {
            function print(self) -> string throws never
        }
        class Widget {
            name: string
        }
        implements<T extends Widget> Printable for T {
            function print(self) -> string { return "w" }
        }
        "#,
        "E0145",
    );
}

#[test]
fn implements_for_unknown_target_is_rejected() {
    // The user-facing top type `unknown` denotes "any type" — it has no single
    // concrete implementor for dispatch to recover, so it is rejected like a
    // union/optional/interface (E0138). `unknown` lowers to `Ty::BuiltinUnknown`,
    // which is distinct from the `Ty::Unknown` error-recovery sentinel, so the
    // gate must list it explicitly.
    assert_compile_error_code(
        r#"
        interface Tag {
            function tag(self) -> int throws never
        }
        implements Tag for unknown {
            function tag(self) -> int { return 0 }
        }
        "#,
        "E0138",
    );
}

#[test]
fn implements_for_literal_target_is_rejected() {
    // A literal type (`1`) is a singleton subtype whose values dispatch through
    // their base (`int`), so it cannot implement an interface itself (E0138).
    assert_compile_error_code(
        r#"
        interface Tag {
            function tag(self) -> int throws never
        }
        implement Tag for 1 {
            function tag(self) -> int { return 0 }
        }
        "#,
        "E0138",
    );
}

#[test]
fn implements_for_enum_variant_target_is_rejected() {
    // An enum variant (`Color.Red`) is likewise a singleton subtype: its values
    // dispatch through the base enum (`Color`), so the variant cannot implement
    // an interface on its own (E0138). Implementing for the enum is allowed.
    assert_compile_error_code(
        r#"
        enum Color { Red, Green, Blue }
        interface Tag {
            function tag(self) -> int throws never
        }
        implement Tag for Color.Red {
            function tag(self) -> int { return 0 }
        }
        "#,
        "E0138",
    );
}

#[test]
fn implements_for_concrete_container_target_is_allowed() {
    // A concrete type constructor (`T[]`) is a valid `for` target — the gate only
    // rejects unions / optionals / interfaces / `unknown`, not list/map/class.
    // Asserts *zero* compile errors (not just the E0112–E0132 interface range) so
    // the concreteness gate's E0138 is covered too.
    assert_zero_compile_errors(
        r#"
        interface Tag {
            function tag(self) -> int throws never
        }
        implements<T> Tag for T[] {
            function tag(self) -> int { return 0 }
        }
    "#,
    );
}

// ── Group: Blanket implementations — Phase 2 (Form 1 runtime) ─────────────

#[test]
fn generic_rule_for_list_receiver_overlaps_concrete_list() {
    assert_compile_error_code(
        r#"
        interface Label {
            function label(self) -> string throws never
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
            function display(self) -> string throws never
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
            function display(self) -> string throws never
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
            function get(self) -> T throws never
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

#[test]
fn unified_rule_repeated_type_vars_reject_conflicting_args() {
    assert_compile_error_contains(
        r#"
        interface Same {
            function tag(self) -> string throws never
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

#[test]
fn form2_does_not_apply_when_bound_not_satisfied() {
    assert_compile_error_contains(
        r#"
        interface Named {
            name: string
        }
        interface Printable {
            function display(self) -> string throws never
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

#[test]
fn bounded_generic_function_type_annotation_is_rejected() {
    // A function *type* annotation cannot declare generic parameters; the valid
    // form is the un-annotated `let method = MyInterface.myMethod`.
    assert_compile_error_contains(
        r#"
        interface MyInterface {
            function myMethod(self) -> int throws never
        }
        function main() -> void {
            let method : <T extends MyInterface>(T) -> int = MyInterface.myMethod
        }
    "#,
        "generic parameters",
    );
}

#[test]
fn parenthesized_generic_apply_is_rejected() {
    // Only a *bare* path reference may be specialized into a value (`foo<int>`);
    // a parenthesized base `(foo)<int>` is rejected even though the inner is a
    // path. (`foo<int>` and `foo<int>(x)` remain valid — see the explicit-type-arg
    // tests.)
    assert_compile_error_contains(
        r#"
        function identity<T>(x: T) -> T { x }
        function caller() -> string {
            let f = (identity)<int>
            return "ok"
        }
        "#,
        "function reference",
    );
}

#[test]
fn inferred_interface_method_reference_enforces_receiver_bound() {
    assert_no_compile_errors(
        r#"
        interface MyInterface {
            function myMethod(self) -> int throws never
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
            function myMethod(self) -> int throws never
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

#[test]
fn form1_bounded_generic_receiver_rejects_when_bound_not_satisfied() {
    let source = r#"
    interface Named {
        name: string
    }
    interface Printable {
        function display(self) -> string throws never
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
            function display(self) -> string throws never
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
            function display(self) -> string throws never
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
fn overlapping_complementary_generic_args_are_e0132() {
    // The symmetric overlap unifier finds the common instance `Pair<string, int>`
    // even though the shared variables sit in complementary positions. A
    // one-directional matcher would miss this.
    assert_compile_error_code(
        r#"
        interface Printable {
            function display(self) -> string throws never
        }
        class Pair<A, B> {
            first: A
            second: B
        }
        implements<T> Printable for Pair<T, int> {
            function display(self) -> string { return "t,int" }
        }
        implements<U> Printable for Pair<string, U> {
            function display(self) -> string { return "string,u" }
        }
        "#,
        "E0132",
    );
}

#[test]
fn non_overlapping_disjoint_generic_args_are_ok() {
    // No common instance: the first arg is `int` in one impl and `string` in the
    // other, so the subjects never unify.
    assert_no_interface_errors(
        r#"
        interface Printable {
            function display(self) -> string throws never
        }
        class Pair<A, B> {
            first: A
            second: B
        }
        implements<T> Printable for Pair<int, T> {
            function display(self) -> string { return "int,t" }
        }
        implements<U> Printable for Pair<string, U> {
            function display(self) -> string { return "string,u" }
        }
        "#,
    );
}

#[test]
fn overlapping_in_body_and_out_of_body_generic_rules_are_e0132() {
    assert_compile_error_code(
        r#"
        interface Printable {
            function display(self) -> string throws never
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
fn cross_file_overlapping_impls_are_e0132() {
    // Coherence is per-package, not per-file: a blanket impl in one file and a
    // concrete impl in another (same package) still conflict. The old per-file
    // check missed this because each file holds only one of the two impls.
    assert_compile_error_contains_multi(
        &[
            (
                "a.baml",
                r#"
                interface Printable {
                    function display(self) -> string throws never
                }
                class Box<T> { value: T }
                implements<T> Printable for Box<T> {
                    function display(self) -> string { return "box" }
                }
                "#,
            ),
            (
                "b.baml",
                r#"
                implements Printable for Box<int> {
                    function display(self) -> string { return "int box" }
                }
                "#,
            ),
        ],
        "E0132",
    );
}

#[test]
fn cross_file_non_overlapping_impls_are_ok() {
    // Two impls of the same interface for distinct classes never conflict, even
    // when split across files in the same package.
    assert_no_compile_errors_multi(&[
        (
            "a.baml",
            r#"
            interface Printable {
                function display(self) -> string throws never
            }
            class Apple {}
            implements Printable for Apple {
                function display(self) -> string { return "apple" }
            }
            "#,
        ),
        (
            "b.baml",
            r#"
            class Banana {}
            implements Printable for Banana {
                function display(self) -> string { return "banana" }
            }
            "#,
        ),
    ]);
}

#[test]
fn form2_overlap_with_form2_is_e0132() {
    assert_compile_error_code(
        r#"
        interface Named {
            name: string
        }
        interface Printable {
            function display(self) -> string throws never
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
            function display(self) -> string throws never
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
            function display(self) -> string throws never
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
            function display(self) -> string throws never
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

// `User` implements `Named`, so the blanket `impl<T: Named> Printable for T`
// applies to `User` and overlaps the concrete `impl Printable for User` — the
// ground subject `User` satisfies the bound, so this is a precise overlap.
#[test]
fn bounded_type_var_rule_overlaps_concrete_satisfying_bound() {
    assert_compile_error_code(
        r#"
        interface Named {
            name: string
        }
        interface Printable {
            function display(self) -> string throws never
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

// `User` does not implement `Named` in this package, so the blanket
// `impl<T: Named> Printable for T` cannot apply to `User` and the two impls are
// disjoint. Deciding this here is sound because `User` and `Named` are both
// local: the orphan rule forbids any dependent package from adding `impl Named
// for User`, so `User` can never gain `Named` and the impls can never collide.
#[test]
fn bounded_type_var_rule_disjoint_from_concrete_not_satisfying_bound() {
    assert_no_interface_errors(
        r#"
        interface Named {
            name: string
        }
        interface Printable {
            function display(self) -> string throws never
        }
        class User {
            name: string
        }
        implements Printable for User {
            function display(self) -> string { return "user" }
        }
        implements<T extends Named> Printable for T {
            function display(self) -> string { return "named" }
        }
        "#,
    );
}

// Two blanket impls bounded by *different* interfaces conflict: without negative
// impls we cannot prove no type satisfies both, so they are assumed to overlap.
#[test]
fn distinct_bounded_blankets_overlap_e0132() {
    assert_compile_error_code(
        r#"
        interface A { a: int }
        interface B { b: int }
        interface Printable { function display(self) -> string throws never }
        implements<T extends A> Printable for T {
            function display(self) -> string { return "a" }
        }
        implements<T extends B> Printable for T {
            function display(self) -> string { return "b" }
        }
        "#,
        "E0132",
    );
}

// An unbounded blanket and a concrete impl of the same interface overlap: the
// blanket covers the concrete type.
#[test]
fn unbounded_blanket_overlaps_concrete_e0132() {
    assert_compile_error_code(
        r#"
        interface Printable { function display(self) -> string throws never }
        class Widget {}
        implements<T> Printable for T {
            function display(self) -> string { return "any" }
        }
        implements Printable for Widget {
            function display(self) -> string { return "widget" }
        }
        "#,
        "E0132",
    );
}

// A homogeneous blanket `Pair<T, T>` and a heterogeneous ground `Pair<int,
// string>` have no common instance (`T` cannot be both `int` and `string`), so
// they are disjoint and do not overlap.
#[test]
fn homogeneous_blanket_disjoint_from_heterogeneous_ground() {
    assert_no_interface_errors(
        r#"
        interface Printable { function display(self) -> string throws never }
        class Pair<A, B> { first: A second: B }
        implements<T> Printable for Pair<T, T> {
            function display(self) -> string { return "tt" }
        }
        implements Printable for Pair<int, string> {
            function display(self) -> string { return "is" }
        }
        "#,
    );
}

// Containers are non-fundamental (like Rust's `Vec`): a local type nested inside
// an array does not anchor an impl, so implementing a foreign interface for
// `Local[]` violates the orphan rule.
#[test]
fn orphan_rejects_foreign_interface_for_local_array() {
    assert_compile_error_code(
        r#"
        class Local {}
        implement baml.ops.Equals for Local[] {}
        "#,
        "E0139",
    );
}

// As above for `map<K, V>` — also a non-fundamental container, so a local type
// nested in it does not anchor a foreign-interface impl.
#[test]
fn orphan_rejects_foreign_interface_for_map_of_local() {
    assert_compile_error_code(
        r#"
        class Local {}
        implement baml.ops.Equals for map<string, Local> {}
        "#,
        "E0139",
    );
}

// Overlap where the shared variable sits in the interface type-arg on one side
// (`P<T>`) and is pinned to a ground type on the other (`P<int>`): both realize
// `P<int>` at the common instance `Box<int>`.
#[test]
fn shared_var_across_interface_arg_and_for_type_overlaps_e0132() {
    assert_compile_error_code(
        r#"
        interface P<X> { function p(self) -> string throws never }
        class Box<T> { v: T }
        implements<T> P<T> for Box<T> {
            function p(self) -> string { return "t" }
        }
        implements<U> P<int> for Box<U> {
            function p(self) -> string { return "int" }
        }
        "#,
        "E0132",
    );
}

// Two impls whose for-types fail to resolve must not additionally report an
// overlap — the unresolved-type errors are the only relevant diagnostics. (An
// unresolved for-type lowers to `Ty::Unknown`, which must never unify.)
//
// Asserts both halves so the test can't pass vacuously: the unresolved-type
// diagnostics (one E0002 per bad target) must be present, and the overlap
// diagnostic (E0132) must be absent.
#[test]
fn malformed_impls_do_not_spuriously_overlap() {
    let errors = collect_compile_errors(
        r#"
        interface Marker {}
        implement Marker for Nonexistent1 {}
        implement Marker for Nonexistent2 {}
        "#,
    );
    let unresolved: Vec<_> = errors.iter().filter(|e| e.starts_with("[E0002]")).collect();
    assert_eq!(
        unresolved.len(),
        2,
        "expected exactly two unresolved-type errors (one per malformed target); got:\n  {}",
        errors.join("\n  ")
    );
    assert!(
        !errors.iter().any(|e| e.starts_with("[E0132]")),
        "unresolved for-types must not be reported as an overlap; got:\n  {}",
        errors.join("\n  ")
    );
}

/// An unresolved interface type-argument in an `implements` clause must be
/// reported EXACTLY ONCE. `impl_data` owns it; the implements-target validator
/// in the LSP must not re-emit it (otherwise the error appears two or three
/// times — the duplication an `.any()`-based assertion never catches).
#[test]
fn implements_target_arg_error_is_reported_exactly_once() {
    let in_body = collect_compile_errors(
        r#"
        interface Container<T> {
            function size(self) -> int throws never
        }
        class Box {
            items: int[]
            implements Container<DoesNotExist> {
                function size(self) -> int { return 0 }
            }
        }
        "#,
    );
    assert_eq!(
        in_body
            .iter()
            .filter(|e| e.contains("DoesNotExist"))
            .count(),
        1,
        "in-body interface arg error must be reported once; got:\n  {}",
        in_body.join("\n  ")
    );

    let out_of_body = collect_compile_errors(
        r#"
        interface Container<T> {
            function size(self) -> int throws never
        }
        class Cat {
            name: string
        }
        implement Container<DoesNotExist> for Cat {
            function size(self) -> int { return 0 }
        }
        "#,
    );
    assert_eq!(
        out_of_body
            .iter()
            .filter(|e| e.contains("DoesNotExist"))
            .count(),
        1,
        "out-of-body interface arg error must be reported once; got:\n  {}",
        out_of_body.join("\n  ")
    );
}

/// When BOTH the interface target and the for-target are unresolved, the
/// for-target error must still be reported — `impl_data` resolves the interface
/// first, but it carries the for-target diagnostic into the failure rather than
/// dropping it.
#[test]
fn unresolved_for_target_reported_even_when_interface_unresolved() {
    let errors = collect_compile_errors(
        r#"
        implement BadInterface for AlsoMissing {}
        "#,
    );
    assert!(
        errors.iter().any(|e| e.contains("AlsoMissing")),
        "the unresolved for-target must not be dropped when the interface is also \
         unresolved; got:\n  {}",
        errors.join("\n  ")
    );
}

// `Box<int | T>` and `Box<int | string | bool>` provably overlap: instantiating
// `T = string | bool` makes them the same realized type. An unbounded variable
// can stand for a union, so it absorbs the extra members — a definite overlap,
// not an indeterminate one (it doesn't matter whether any code uses that `T`).
#[test]
fn variable_in_union_arg_overlaps_via_union_expansion() {
    assert_compile_error_contains(
        r#"
        interface Marker {}
        class Box<X> { v: X }
        implements<T> Marker for Box<int | T> {}
        implements Marker for Box<int | string | bool> {}
        "#,
        "overlapping interface implementations",
    );
}

// A definite overlap keeps the plain "overlapping" wording (not the indeterminate
// one) — confirms the tri-state labels the two cases distinctly.
#[test]
fn definite_overlap_reports_overlapping_message() {
    assert_compile_error_contains(
        r#"
        interface Marker {}
        class Widget {}
        implements<T> Marker for T {}
        implements Marker for Widget {}
        "#,
        "overlapping interface implementations",
    );
}

// `Box<Pair<T, int>>` and `Box<Pair<string, U>>` overlap at the common instance
// `Box<Pair<string, int>>` — variables in complementary positions of a nested
// generic still unify.
#[test]
fn nested_generic_complementary_args_overlap_e0132() {
    assert_compile_error_code(
        r#"
        interface Marker {}
        class Box<X> { v: X }
        class Pair<A, B> { first: A second: B }
        implements<T> Marker for Box<Pair<T, int>> {}
        implements<U> Marker for Box<Pair<string, U>> {}
        "#,
        "E0132",
    );
}

// `Box<Pair<int, T>>` and `Box<Pair<string, U>>` are disjoint: the first nested
// argument is `int` vs `string`, which can never coincide.
#[test]
fn nested_generic_disjoint_ground_arg_is_ok() {
    assert_no_interface_errors(
        r#"
        interface Marker {}
        class Box<X> { v: X }
        class Pair<A, B> { first: A second: B }
        implements<T> Marker for Box<Pair<int, T>> {}
        implements<U> Marker for Box<Pair<string, U>> {}
        "#,
    );
}

// A union nested deep in a generic argument still overlaps via expansion:
// `Pair<int, string | T>` and `Pair<int, string | bool>` coincide at `T = bool`.
#[test]
fn nested_union_in_generic_arg_overlaps_via_expansion() {
    assert_compile_error_code(
        r#"
        interface Marker {}
        class Box<X> { v: X }
        class Pair<A, B> { first: A second: B }
        implements<T> Marker for Box<Pair<int, string | T>> {}
        implements Marker for Box<Pair<int, string | bool>> {}
        "#,
        "E0132",
    );
}

// Three levels deep (`Box` ▷ `Pair` ▷ `[]`) with variables on both sides:
// `Box<Pair<T[], int>>` and `Box<Pair<string[], U>>` overlap at
// `Box<Pair<string[], int>>`.
#[test]
fn deeply_nested_generic_with_list_overlaps_e0132() {
    assert_compile_error_code(
        r#"
        interface Marker {}
        class Box<X> { v: X }
        class Pair<A, B> { first: A second: B }
        implements<T> Marker for Box<Pair<T[], int>> {}
        implements<U> Marker for Box<Pair<string[], U>> {}
        "#,
        "E0132",
    );
}

// A nested union whose rigid member can't be matched is disjoint even with a
// variable present: `int | T` can never equal `string | float` (no `int` on the
// right, and `T` cannot put one there).
#[test]
fn nested_union_in_generic_arg_disjoint_is_ok() {
    assert_no_interface_errors(
        r#"
        interface Marker {}
        class Box<X> { v: X }
        class Pair<A, B> { first: A second: B }
        implements<T> Marker for Box<Pair<int | T, bool>> {}
        implements Marker for Box<Pair<string | float, bool>> {}
        "#,
    );
}

// Idempotent collapse: `Box<T[] | U[] | W[]>` and `Box<Foo[] | Bar[]>` coincide at
// `T=Foo, U=Bar, W=Foo` (the third member collapses onto the first). The covering
// solver catches this; an injective matcher (the old model) wrongly accepted both.
#[test]
fn union_collapse_overlaps_e0132() {
    assert_compile_error_code(
        r#"
        interface Marker {}
        class Box<X> { v: X }
        class Foo { x: int }
        class Bar { y: int }
        implements<T, U, W> Marker for Box<T[] | U[] | W[]> {}
        implements Marker for Box<Foo[] | Bar[]> {}
        "#,
        "E0132",
    );
}

// `int` and the literal `1` are distinct types (generics are invariant), so `Box<int>`
// and `Box<1>` are disjoint — no coherence conflict.
#[test]
fn box_int_vs_box_literal_is_disjoint() {
    assert_no_compile_errors(
        r#"
        interface Marker {}
        class Box<X> { v: X }
        implements Marker for Box<int> {}
        implements Marker for Box<1> {}
        "#,
    );
}

// A finite literal union is a strict subset of its base under invariance: `1 | 2` is not
// `int`, so `Box<1 | 2>` and `Box<int>` are disjoint.
#[test]
fn literal_union_subset_of_base_is_disjoint() {
    assert_no_compile_errors(
        r#"
        interface Marker {}
        class Box<X> { v: X }
        implements Marker for Box<1 | 2> {}
        implements Marker for Box<int> {}
        "#,
    );
}

// A complete finite base folds back to the base: `true | false` is `bool`, so these two
// blocks are the *same* type and conflict.
#[test]
fn complete_bool_union_folds_and_overlaps_e0132() {
    assert_compile_error_code(
        r#"
        interface Marker {}
        class Box<X> { v: X }
        implements Marker for Box<true | false> {}
        implements Marker for Box<bool> {}
        "#,
        "E0132",
    );
}

// All of an enum's variants fold back to the enum: `Cmp.Less | Cmp.Equal | Cmp.More` is
// `Cmp`, so `Box<…>` and `Box<Cmp>` conflict. (Soundness, not just precision: without
// folding the two would be wrongly accepted as disjoint.)
#[test]
fn complete_enum_union_folds_and_overlaps_e0132() {
    assert_compile_error_code(
        r#"
        interface Marker {}
        class Box<X> { v: X }
        enum Cmp { Less Equal More }
        implements Marker for Box<Cmp.Less | Cmp.Equal | Cmp.More> {}
        implements Marker for Box<Cmp> {}
        "#,
        "E0132",
    );
}

// A coupled union too large for the overlap search to resolve within its budget yields
// the *indeterminate* rejection (a pigeonhole: five variables cannot realize six
// distinct members, but proving it is NP-hard). The impls are still rejected, with the
// "too complex — simplify" message rather than a definite-overlap one.
#[test]
fn intractable_union_overlap_asks_to_simplify() {
    assert_compile_error_contains(
        r#"
        interface Marker {}
        class Box<X> { v: X }
        class Pair<A, B> { f: A s: B }
        class A0 { x: int } class A1 { x: int } class A2 { x: int }
        class A3 { x: int } class A4 { x: int } class A5 { x: int }
        implements<T0, T1, T2, T3, T4> Marker for Box<
            Pair<T0, T0> | Pair<T1, T1> | Pair<T2, T2> | Pair<T3, T3> | Pair<T4, T4>
        > {}
        implements Marker for Box<
            Pair<A0, A0> | Pair<A1, A1> | Pair<A2, A2> | Pair<A3, A3> | Pair<A4, A4> | Pair<A5, A5>
        > {}
        "#,
        "simplify",
    );
}

// `Box<C | T>` and `Box<C>` overlap at `T = C` (idempotency collapses `C | C` to `C`),
// the union-vs-non-union analogue of the collapse the `(Union, Union)` path already
// rejects. Before routing the non-union operand through covering, this was a wrong
// `No` — two conflicting impls silently accepted.
#[test]
fn union_member_vs_single_collapse_overlaps_e0132() {
    assert_compile_error_code(
        r#"
        interface Marker {}
        class Box<X> { v: X }
        class C {}
        implements<T> Marker for Box<C | T> {}
        implements Marker for Box<C> {}
        "#,
        "E0132",
    );
}

// `Box<D | T>` and `Box<C>` (with `C ≠ D`) are disjoint: the union always contains `D`,
// which no `T` removes, so it can never equal `C`. Covering keeps this precise (no error),
// not a conservative over-reject.
#[test]
fn union_member_vs_unrelated_single_is_disjoint() {
    assert_no_compile_errors(
        r#"
        interface Marker {}
        class Box<X> { v: X }
        class C {}
        class D {}
        implements<T> Marker for Box<D | T> {}
        implements Marker for Box<C> {}
        "#,
    );
}

// A blanket `Box<T>` overlaps `Box<unknown>` at `T = unknown` (the inhabited top type).
// The old solver lumped `unknown` in with the error sentinel and wrongly accepted both.
#[test]
fn box_unknown_vs_blanket_overlaps_e0132() {
    assert_compile_error_code(
        r#"
        interface Marker {}
        class Box<X> { v: X }
        implements<T> Marker for Box<T> {}
        implements Marker for Box<unknown> {}
        "#,
        "E0132",
    );
}

// `Box<unknown>` and `Box<int>` are disjoint: under invariance `unknown` is a distinct
// atomic type (only an `unknown` value inhabits `Box<unknown>`), matching the runtime
// resolver. So implementing for both is allowed.
#[test]
fn box_unknown_vs_box_int_is_disjoint() {
    assert_no_compile_errors(
        r#"
        interface Marker {}
        class Box<X> { v: X }
        implements Marker for Box<unknown> {}
        implements Marker for Box<int> {}
        "#,
    );
}

// `Box<T[] | R>` (with `T extends HasName`) and `Box<Cat[] | Dog[]>` overlap at the
// common instance `Box<Cat[] | Dog[]>` via `T = Dog` (which satisfies the bound) and
// `R = Cat[]`. The bound check must not disprove this against the *other* cover witness
// (`T = Cat`, which fails the bound) — the verdict must not depend on union member order.
#[test]
fn bounded_union_member_overlaps_concrete_union_e0132() {
    assert_compile_error_code(
        r#"
        interface HasName {}
        interface Marker {}
        class Box<X> { v: X }
        class Cat {}
        class Dog {}
        implements HasName for Dog {}
        implements<T extends HasName, R> Marker for Box<T[] | R> {}
        implements Marker for Box<Cat[] | Dog[]> {}
        "#,
        "E0132",
    );
}

// The same overlap, with the concrete union's members in the opposite order — the
// bound-satisfying witness (`Dog`) now comes first. Both orders must report the overlap.
#[test]
fn bounded_union_member_overlaps_concrete_union_reversed_e0132() {
    assert_compile_error_code(
        r#"
        interface HasName {}
        interface Marker {}
        class Box<X> { v: X }
        class Cat {}
        class Dog {}
        implements HasName for Dog {}
        implements<T extends HasName, R> Marker for Box<T[] | R> {}
        implements Marker for Box<Dog[] | Cat[]> {}
        "#,
        "E0132",
    );
}

// A top-level type-alias for-type must not evade coherence. Before the valid-subject
// gate expanded aliases, `impl I for C` + `impl I for AliasC` (`type AliasC = C`)
// slipped past both E0132 and E0114, leaving two impls for the same concrete type.
#[test]
fn alias_for_type_overlaps_concrete_e0132() {
    assert_compile_error_code(
        r#"
        interface Marker {}
        class C {}
        type AliasC = C
        implement Marker for C {}
        implement Marker for AliasC {}
        "#,
        "E0132",
    );
}

#[test]
fn alias_of_alias_for_type_overlaps_e0132() {
    assert_compile_error_code(
        r#"
        interface Marker {}
        class C {}
        type A = C
        type B = A
        implement Marker for C {}
        implement Marker for B {}
        "#,
        "E0132",
    );
}

#[test]
fn two_distinct_aliases_of_same_class_overlap_e0132() {
    assert_compile_error_code(
        r#"
        interface Marker {}
        class C {}
        type A = C
        type B = C
        implement Marker for A {}
        implement Marker for B {}
        "#,
        "E0132",
    );
}

// The alias must not defeat an otherwise-working blanket-vs-concrete overlap; this
// needs `unify_into` to see through the alias for the variable-binding structural
// match, not only the exact-equality fast path.
#[test]
fn alias_for_type_overlaps_blanket_e0132() {
    assert_compile_error_code(
        r#"
        interface Marker {}
        class Box<X> { v: X }
        type BI = Box<int>
        implements<T> Marker for Box<T> {}
        implement Marker for BI {}
        "#,
        "E0132",
    );
}

// Control: aliases to DIFFERENT concrete types are genuinely disjoint — no error
// (the alias expansion must not over-merge them).
#[test]
fn aliases_to_distinct_classes_are_disjoint() {
    assert_no_compile_errors(
        r#"
        interface Marker {}
        class C {}
        class D {}
        type AliasC = C
        type AliasD = D
        implement Marker for AliasC {}
        implement Marker for AliasD {}
        "#,
    );
}

// A `Future` for-type is not implementable: its value/error args can't be carried by
// the runtime impl registry's `TyTemplate`, so a generic `Future<T>` would bake an
// undispatchable rule. Removed from the implementable whitelist so it errors (E0138)
// rather than silently vanishing.
#[test]
fn future_for_type_is_rejected_e0138() {
    assert_compile_error_code(
        r#"
        interface Marker {}
        implement Marker for baml.future.Future<int, string> {}
        "#,
        "E0138",
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
                    function display(self) -> string throws never
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
                    function display(self) -> string throws never
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
                    function sound(self) -> string throws never
                    function describe(self) -> string throws never {
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
                    function name(self) -> string throws never
                }
                "#,
        ),
        (
            "ns_b/b.baml",
            r#"
                interface Greeter {
                    function greet(self) -> string throws never
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
                    function name(self) -> string throws never
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

    // An unrooted cross-namespace `a.Named` does not resolve (the absolute form
    // `root.a.Named` is required), so it surfaces as a general unresolved-type
    // error rather than a dedicated implements-target diagnostic.
    assert_compile_error_contains_multi(files, "unresolved type: a.Named");
}

#[test]
fn qualified_generic_constructor_preserves_concrete_type_in_diagnostics() {
    let files = &[
        (
            "main.baml",
            r#"
                interface Printable<T> {
                    function display(self) -> string throws never
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

#[test]
fn bounded_type_var_rule_conservatively_overlaps_generic_class_rule() {
    assert_compile_error_code(
        r#"
        interface Named {
            name: string
        }
        interface Printable {
            function display(self) -> string throws never
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
            function display(self) -> string throws never
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
// Each test below is a regression guard pinning the corrected behavior for a
// finding from that sweep; the doc comment on each names the original defect it
// guards against. All of these PASS now — positive cases run the program
// end-to-end and assert the concrete result; "must-reject" cases assert the
// diagnostic that fires. Numbers map to _plan/baml_interface_findings.md.
// Finding #30 (a too-weak existing test) is covered by `fuzz_bug29_*` below,
// which asserts the canonical `let d: Dog =>` form narrows.
//
// EXCEPTION: findings #1 and #2 (`fuzz_bug01_*`, `fuzz_bug02_*`) remain
// `#[ignore]`d — they need interface methods as first-class values
// (`let f = Interface.method` with dynamic dispatch), which is not implemented.
// ═══════════════════════════════════════════════════════════════════════════

/// Finding #1 [crash]: Interface method reference on required (abstract) method crashes at runtime
#[ignore = "unsupported: taking an interface method as a first-class value \
            (`let f = Interface.method`) and calling it with dynamic dispatch \
            needs a synthesized dispatcher thunk — not implemented"]
#[tokio::test]
async fn fuzz_bug01_method_ref_required_method_crashes() {
    let output = baml_test!(
        r##"interface Animal {
    function speak(self) -> string throws never
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
    function greet(self) -> string throws never {
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

/// Finding #8: a generic class implementing the same single-`T` interface for
/// both of its type params (`Getter<L>` + `Getter<R>`) overlaps at the diagonal
/// `Pair<T, T>` (both realize `Getter<T>`), so the impls are rejected and the
/// mis-dispatch the finding describes can no longer arise.
#[test]
fn fuzz_bug08_generic_class_overlapping_type_arg_impls_rejected() {
    assert_compile_error_code(
        r##"interface Getter<T> {
    function get(self) -> T throws never
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
"##,
        "E0132",
    );
}

/// Finding #9 [wrongly-accepted]: Unqualified method call on class implementing same generic interface with different type args silently picks first impl (no E0121)
#[test]
fn fuzz_bug09_same_generic_iface_diff_typeargs_unqualified_call_ambiguous() {
    assert_compile_error_code(
        r##"interface Converter<T> {
    function convert(self) -> T throws never
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

/// Finding #12: `Slot<L>` + `Slot<R>` on `GenPair<L, R>` overlap at the diagonal
/// `GenPair<T, T>` (both realize `Slot<T>`), so the impls are rejected and the
/// first-block mis-dispatch the finding describes can no longer arise.
#[test]
fn fuzz_bug12_generic_pair_overlapping_type_arg_impls_rejected() {
    assert_compile_error_code(
        r##"interface Slot<T> {
    function get(self) -> T throws never
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
    // Each interface view dispatches to its own impl block:
    // lv == 42 (Slot<int>) && rv == "world" (Slot<string>).
    return lv == 42 && rv == "world"
}
"##,
        "E0132",
    );
}

/// Finding #13: the field-link form of the same overlap — `Slot<L>` + `Slot<R>`
/// on `GenPair<L, R>` collide at the diagonal `GenPair<T, T>`, so the impls are
/// rejected.
#[test]
fn fuzz_bug13_generic_field_link_overlapping_type_arg_impls_rejected() {
    assert_compile_error_code(
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
    // Each field-link view selects the impl block matching its type arg:
    // i.value == 7 (Slot<int>) && s.value == "seven" (Slot<string>).
    return i.value == 7 && s.value == "seven"
}
"##,
        "E0132",
    );
}

/// Finding #14: the `.as<>` projection form of the same overlap — `Slot<L>` +
/// `Slot<R>` on `GenPair<L, R>` collide at the diagonal `GenPair<T, T>`, so the
/// impls are rejected before any projection runs.
#[test]
fn fuzz_bug14_generic_overlapping_type_arg_impls_reject_as_projection() {
    assert_compile_error_code(
        r##"interface Slot<T> {
    function get(self) -> T throws never
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
"##,
        "E0132",
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

/// Finding #22: the old `x.Interface.method()` projection syntax is no longer
/// special-cased — `Container` is not a member of `IntBox`, so it is a plain
/// E0007 no-member error.
#[test]
fn fuzz_bug22_old_projection_syntax_is_no_member_error() {
    assert_compile_error_contains(
        r##"interface Container<T> {
    function get(self) -> T throws never
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
        "has no member `Container`",
    );
}

/// Finding #25 [bad-error]: Two default methods with same name give E0007 (method not found) instead of E0121 (ambiguous)
#[test]
fn fuzz_bug25_two_same_named_default_methods_are_ambiguous() {
    assert_compile_error_code(
        r##"interface Alpha {
    function tag(self) -> string throws never { return "alpha" }
}
interface Beta {
    function tag(self) -> string throws never { return "beta" }
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
    function process(self) -> string throws never { return "DEFAULT" }
}
interface WithRequired {
    function process(self) -> string throws never
}

class Impl {
    implements WithDefault {}
    implements WithRequired {
        function process(self) -> string { return "REQUIRED" }
    }
}

function main() -> string {
    let x = Impl {}
    return x.process()  // E0121: ambiguous between the default and required `process`
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
    function convert(self) -> T throws never
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
    return m.convert()  // E0121: ambiguous between Converter<int> and Converter<string>
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
    // `requires DoesNotExist` names nothing at all. The diagnostic must be the
    // general unresolved-type error (E0002 `unresolved type: DoesNotExist`),
    // not "is not an interface" (E0133) — the latter wrongly implies the symbol
    // exists with the wrong kind.
    let errors = collect_compile_errors(
        r#"
        interface Person requires DoesNotExist {
            name: string
        }
        "#,
    );
    assert!(
        errors.iter().any(|e| e.starts_with("[E0002]")),
        "expected an E0002 unresolved-type error, got:\n  {}",
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
            function size(self) -> int throws never
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
            function size(self) -> int throws never
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
            r#"interface Animal { function speak(self) -> string throws never }"#,
        ),
        (
            "ns_farm/farm.baml",
            r#"interface Animal { function speak(self) -> string throws never }"#,
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
            r#"interface Animal { function speak(self) -> string throws never }"#,
        ),
        (
            "ns_farm/farm.baml",
            r#"interface Animal { function speak(self) -> string throws never }"#,
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
// Like the `fuzz_*` suite above, each test is a regression guard that PASSES
// now; its doc comment names the original defect it guards against. Tests
// suffixed `_pins` pin behavior that was already correct (sometimes subtle or
// surprising) so a regression is caught.
// ═══════════════════════════════════════════════════════════════════════════

// ── Formerly crashes (type-checker-accepted code that panicked the VM) ───────

/// wf3 #3 [was: crash/unsound]: a phantom impl type param
/// (`implements<T> Tagged<T> for Holder` where `T` appears only in the interface
/// args) is now rejected at compile time — previously it compiled, was accepted
/// as `Tagged<int|string|bool>`, and crashed the VM.
/// `_plan/wf3/generics-bounds-blanket/p13c_phantom_single.baml`
/// Rust-faithful (E0207): an impl type parameter is *constrained* if it appears in the
/// implemented interface reference (`Tagged<T>`), not only the self type — so
/// `implements<T> Tagged<T> for Holder` is a valid blanket over the interface parameter
/// (mirrors Rust's `impl<T> From<T> for MyType`). The existential `Tagged<int>` pins
/// `T = int`, so there is no erasure at the use site.
#[test]
fn wf3_impl_type_param_in_interface_args_is_accepted() {
    assert_zero_compile_errors(
        r#"
        interface Tagged<T> { function tag(self) -> string throws never }
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
}

// ── High-severity wrong-result / soundness ───────────────────────────────────

/// wf3 #5: `Slot<L>` + `Slot<R>` on `GenPair<L, R>` (here with a default method)
/// overlap at the diagonal `GenPair<T, T>`, so the impls are rejected — the
/// interface-view default-method dispatch the finding describes can no longer
/// arise. `_plan/wf3/generics-core/gen_pair_default_selfcall.baml`
#[test]
fn wf3_generic_default_method_overlapping_type_arg_impls_rejected() {
    assert_compile_error_code(
        r#"
        interface Slot<T> {
            function get(self) -> T throws never
            function describe(self) -> T throws never {
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
        "#,
        "E0132",
    );
}

/// wf3 #8 [was: high/soundness]: a `-> Self` method in `Box`'s implements block
/// has `Self = Box`; returning a `Cup` is now a compile error. Previously it
/// compiled and a `Box`-typed value backed by a runtime `Cup` read a
/// non-existent field. `_plan/wf3/self-types/self_wrong_class_field_access.baml`
#[test]
fn wf3_self_return_wrong_concrete_class_is_rejected() {
    let errors = collect_compile_errors(
        r#"
        interface Cloneable {
            function clone(self) -> Self throws never
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
            r#"interface Animal { function speak(self) -> string throws never }"#,
        ),
        (
            "ns_farm/farm.baml",
            r#"interface Animal { function speak(self) -> string throws never }"#,
        ),
    ];
    assert_compile_error_contains_multi(files, "root.zoo.Animal");
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
        (
            "ns_a/a.baml",
            r#"interface Base { function f(self) -> string throws never }"#,
        ),
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
        (
            "ns_a/a.baml",
            r#"interface A { function f(self) -> string throws never }"#,
        ),
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

/// wf3 #13 [medium]: calling `.speak()` on `Animal | Swimmer` must be rejected.
/// Union member access is valid only through a single interface that *every* arm
/// shares and that declares the member. `Animal` declares `speak` but `Swimmer`
/// does not, so the arms share no common interface that declares `speak`. The
/// diagnostic must say exactly that — not falsely claim the `Animal` arm lacks
/// `speak` (it declares it).
/// `_plan/wf3/subtyping-optional-union-match/union_method_on_iface_union.baml`
#[test]
fn wf3_method_on_interface_union_blames_correct_member() {
    let errors = collect_compile_errors(
        r#"
        interface Animal {
            function speak(self) -> string throws never
        }
        interface Swimmer {
            function swim(self) -> string throws never
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
        errors
            .iter()
            .any(|e| e.contains("no common interface that declares")),
        "`.speak()` on `Animal | Swimmer` must be rejected: the arms share no \
         common interface that declares `speak`. Got:\n  {}",
        errors.join("\n  ")
    );
    assert!(
        !errors.iter().any(|e| e.contains("`Animal` has no member")),
        "diagnostic must not falsely claim the `Animal` arm lacks a member — it \
         declares `speak`; the union simply shares no common interface. Got:\n  {}",
        errors.join("\n  ")
    );
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
            function get(self) -> T throws never
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

/// wf3 #20: `Getter<L>` + `Getter<R>` on `Pair<L, R>` overlap at the diagonal
/// `Pair<T, T>` (both realize `Getter<T>`), so the class is rejected with the
/// overlapping-implementations coherence error (E0132) at its declaration —
/// before any call-site collision suggestion can arise.
/// `_plan/wf3/generics-core/gen_mono_collision_unqualified.baml`
#[test]
fn wf3_monomorph_collision_overlapping_impls_rejected() {
    assert_compile_error_contains(
        r#"
        interface Getter<T> {
            function get(self) -> T throws never
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
        "overlapping interface implementations",
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
            r#"interface Base { function f(self) -> string throws never }"#,
        ),
        (
            "ns_b/b.baml",
            r#"interface Derived requires root.a.Ghost {}"#,
        ),
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
        interface Printable { function display(self) -> string throws never }
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
            function speak(self) -> string throws never
        }
        interface Vehicle {
            function drive(self) -> string throws never
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
            function log(self, msg: string) -> string throws never { return msg }
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
            function greet(self) -> string throws never
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

/// wf3 [low/design]: a `throws` narrower than the interface's declaration
/// (`throws NetworkError` where `NetworkError implements IError` and the
/// interface declares `throws IError`) is now allowed by covariance, matching
/// the fact that throwing a subtype at a throw-site is fine (previously E0120).
/// `_plan/wf3/out-of-body-throws/oob_throws_subset.baml`
#[test]
fn wf3_throws_covariant_narrower_is_allowed() {
    assert_no_compile_errors(
        r#"
        interface IError {
            function describe(self) -> string throws never
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
            function greet(self) -> string throws never { return "hi" }
        }
        interface B requires A {
            function bye(self) -> string throws never { return "bye" }
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
        interface A requires B { function fa(self) -> string throws never }
        interface B requires A { function fb(self) -> string throws never }
        class C {
            implements A { function fa(self) -> string { return "a" } }
            implements B { function fb(self) -> string { return "b" } }
        }
        function main() -> string {
            let c: A = C {}
            return c.fa()
        }
        "#,
        "A → B",
    );
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
            function display(self) -> string throws never
        }
        implements Named for int {
            function display(self) -> string { return "int" }
        }
        function main() -> string { return "ok" }
        "#,
        "E0126",
    );
}

/// wf3: a bare generic interface in a type-argument position
/// (`reflect.Type.of<Box>()`, no type args) is an arity error like any other
/// type position — a generic head is written fully explicit or inferred
/// wholesale, never a partial wildcard. (This replaces the old undocumented
/// wildcard-matching behavior; a deliberate every-instantiation reflection
/// query would need its own designed spelling.)
/// `_plan/wf3/generics-reflection/gen_bare_impl_both.baml`
#[tokio::test]
async fn wf3_bare_generic_interface_reflection_is_arity_error() {
    assert_compile_error_contains(
        r#"
        interface Box<T> {
            function get(self) -> T throws never
        }
        class IntBox {
            implements Box<int> {
                function get(self) -> int { return 1 }
            }
        }
        function main() -> bool {
            let bare = reflect.Type.of<Box>()
            return bare.implemented_by(reflect.Type.of<IntBox>())
        }
        "#,
        "type `Box` expects 1 type argument(s), got 0",
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Group: union-fuzz findings (workflow_scratch_files/FINDINGS.md, 2026-06-01)
//
// 17 union-focused interface bugs found by the `baml-interface-union-fuzz`
// workflow. Each `union_fuzz_fNN_*` test asserts the DESIRED behavior, so it
// FAILS today and turns GREEN once the bug is fixed (no edit required). The
// runnable `.baml` repro for each finding lives under
// `workflow_scratch_files/cat_<category>/`. Severities: crash > wrong-result >
// spurious-compile-error > missing-error > bad-diagnostic.
// ─────────────────────────────────────────────────────────────────────────────

/// A method present on every member of a *class-only* union (`A | B`, each
/// declaring its own `execute` directly) is NOT callable on the union. Union
/// member access is valid only through a single interface that *every* arm shares
/// and that declares the member; each class declares its own `execute` (no shared
/// interface at all), so the union shares no common interface that declares
/// `execute` — a compile error.
#[test]
fn union_fuzz_class_only_union_method_is_rejected() {
    assert_compile_error_contains(
        r#"
        class A {
          name: string
          function execute(self) -> string { return "A executes and gets " + self.name }
        }
        class B {
          name: string
          function execute(self) -> string { return "B executes and gets " + self.name }
        }
        function process(input: A | B) -> string { return input.execute() }
        function main() -> string {
          return process(A { name: "Alice" }) + " | " + process(B { name: "Bob" })
        }
        "#,
        "no common interface that declares",
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// PR #3638 review follow-ups: extra union cases surfaced by CodeRabbit/Cursor on
// the F1–F17 fix branch. Each reproduced a real bug before the follow-up fix.
// ─────────────────────────────────────────────────────────────────────────────

/// Calling a method that two interfaces both declare, on a *union* of classes
/// that inherit it, must report the same E0121 ambiguity (with the `.as<I>`
/// hint) the single-class receiver does — not collapse to a misleading
/// `unknown | unknown is not a function` (E0006) leaking the internal sentinel.
#[test]
fn union_fuzz_pr_ambiguous_inherited_method_in_class_union_is_e0121() {
    let errors = collect_compile_errors(
        r#"
        interface A { function f(self) -> string  throws never { return "a" } }
        interface B { function f(self) -> string  throws never { return "b" } }
        class C { implements A {} implements B {} }
        class D { implements A {} implements B {} }
        function g(x: C | D) -> string { return x.f() }
        function main() -> string { return g(C {}) }
        "#,
    );
    assert!(
        errors.iter().any(|e| e.starts_with("[E0121]")),
        "ambiguous inherited method on a class union must be E0121; got:\n  {}",
        errors.join("\n  ")
    );
    assert!(
        !errors
            .iter()
            .any(|e| e.contains("unknown |") || e.contains("is not a function")),
        "must not leak the internal `unknown | unknown` not-a-function form (E0006); got:\n  {}",
        errors.join("\n  ")
    );
}

/// A concrete non-function arm in a union callee must still be reported as
/// not-callable even when another arm is in recovery (`int | <unresolved>`).
/// The E0006-suppression added for ambiguous-method unions must only fire when
/// *every* non-function arm is already in recovery, not on any recovery arm.
#[test]
fn union_fuzz_pr_concrete_arm_not_callable_despite_recovery_arm() {
    let errors = collect_compile_errors(
        r#"
        function f(x: int | DoesNotExist) -> string { return x() }
        function main() -> string { return "ok" }
        "#,
    );
    assert!(
        errors.iter().any(|e| e.starts_with("[E0006]")),
        "the concrete `int` arm of an `int | <recovery>` callee must still be \
         reported not-callable (E0006); got:\n  {}",
        errors.join("\n  ")
    );
}

/// Calling `m` on a union of two *different* interfaces (`A | B`) that each
/// declare their own `m` is rejected, even when one class implements both. Union
/// member access is valid only through a single interface that *every* arm shares
/// and that declares the member; `A.m` and `B.m` are distinct members, so the
/// union shares no common interface that declares `m`.
#[test]
fn union_fuzz_pr_shared_implementor_method_in_iface_union_has_no_common_interface() {
    assert_compile_error_contains(
        r#"
        interface A { function m(self) -> string  throws never { return "from-A" } }
        interface B { function m(self) -> string  throws never { return "from-B" } }
        class C { implements A {} implements B {} }
        function call(x: A | B) -> string { return x.m() }
        function main() -> string { let c: A = C {}; return call(c) }
        "#,
        "no common interface that declares",
    );
}

/// Calling `m` on a union of two *different* interfaces (`A | B`) that each
/// declare their own `m`, with disjoint implementors (`Dog` is `A`, `Cat` is
/// `B`), is still rejected. Union member access is valid only through a single
/// interface that *every* arm shares and that declares the member; `A` and `B`
/// are distinct interfaces, so their `m`s are distinct members and the union
/// shares no common interface that declares `m`.
#[test]
fn union_fuzz_pr_disjoint_implementors_iface_union_is_rejected() {
    assert_compile_error_contains(
        r#"
        interface A { function m(self) -> string  throws never { return "a" } }
        interface B { function m(self) -> string  throws never { return "b" } }
        class Dog { implements A {} }
        class Cat { implements B {} }
        function call(x: A | B) -> string { return x.m() }
        function main() -> string {
          let d: A = Dog {}
          let c: B = Cat {}
          return call(d) + call(c)
        }
        "#,
        "no common interface that declares",
    );
}

/// F3 [crash]: a same-named field declared by two *different* union interfaces
/// (`Animal.id: string`, `Vehicle.id: int`) used to type-check against any target
/// and then abort the VM (`expected map, got instance`). Union member access is
/// valid only through a single interface that *every* arm shares and that declares
/// the member; `Animal.id` and `Vehicle.id` are distinct members, so the union
/// shares no common interface that declares `id` — a clean compile error, never a
/// VM crash.
/// Repro: `cat_iface_iface_union/iface_iface_union_7_same_field_diff_type.baml`
#[test]
fn union_fuzz_f03_conflicting_union_field_has_no_common_interface() {
    assert_compile_error_contains(
        r#"
        interface Animal { id: string }
        interface Vehicle { id: int }
        class Dog { id: string  implements Animal {} }
        class Car { id: int  implements Vehicle {} }
        function pick(b: bool) -> Animal | Vehicle {
          if b { return Dog { id: "rex" } } else { return Car { id: 7 } }
        }
        function main() -> string {
          let u: Animal | Vehicle = pick(false)
          let v: string = u.id
          return v
        }
        "#,
        "no common interface that declares",
    );
}

/// F4 [crash]: `string + int` type-checks (inferred as `string`) but the VM
/// aborts with `cannot apply binary operation: string + int`. The two phases
/// must agree — the type-checker SHOULD reject it. (Symmetric arithmetic hole;
/// surfaced via interface projection in the sibling repro, not interface-only.)
/// Repro: `cat_as_projection_union/as_projection_union_20_plain_int_concat.baml`
#[test]
fn union_fuzz_f04_string_plus_int_is_type_error_not_vm_crash() {
    let errors = collect_compile_errors(
        r#"
        function plainPlus() -> string {
          return "age=" + 4
        }
        function main() -> string {
          return plainPlus()
        }
        "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| e.starts_with("[E0004]") && e.contains("operator `+`")),
        "`string + int` must be rejected with an InvalidBinaryOp (E0004) naming `+` \
         (it used to infer `string` and abort the VM); got:\n  {}",
        errors.join("\n  ")
    );
}

/// F7: calling a method that both arms declare through *different* interfaces
/// (`Animal.speak` / `Vehicle.speak`) on a union `Animal | Vehicle` is a compile
/// error. Union member access is valid only through a single interface that
/// *every* arm shares and that declares the member; `Animal.speak` and
/// `Vehicle.speak` are distinct members, so the union shares no common interface
/// that declares `speak`.
/// Repro: `cat_collection_union/collection_union_9_union_direct_call.baml`
#[test]
fn union_fuzz_f07_shared_method_on_iface_union_is_a_compile_error() {
    assert_compile_error_contains(
        r#"
        interface Animal {
          function speak(self) -> string throws never
        }
        class Dog {
          implements Animal {
            function speak(self) -> string { return "Woof" }
          }
        }
        interface Vehicle {
          function speak(self) -> string throws never
        }
        class Car {
          implements Vehicle {
            function speak(self) -> string { return "Vroom" }
          }
        }
        function main() -> string {
          let v: Animal | Vehicle = Dog {};
          return v.speak();
        }
        "#,
        "no common interface that declares",
    );
}

/// F11 [missing-error]: the soundness face of F3 — reading a field declared by
/// two *different* union interfaces (`Animal.id: string` / `Vehicle.id: int`)
/// used to type-check against ANY target, including `bool`. Union member access
/// is valid only through a single interface that *every* arm shares and that
/// declares the member; `Animal.id` and `Vehicle.id` are distinct members, so the
/// union shares no common interface that declares `id` — rejected at compile time.
/// Repro: `cat_iface_iface_union/iface_iface_union_7_same_field_diff_type.baml`
#[test]
fn union_fuzz_f11_conflicting_union_field_read_is_rejected() {
    assert_compile_error_contains(
        r#"
        interface Animal { id: string }
        interface Vehicle { id: int }
        class Dog { id: string  implements Animal {} }
        class Car { id: int  implements Vehicle {} }
        function readBool(u: Animal | Vehicle) -> bool {
          let v: bool = u.id
          return v
        }
        function main() -> bool { return readBool(Dog { id: "x" }) }
        "#,
        "no common interface that declares",
    );
}

/// F12 [bad-diagnostic]: the diagnostic face of F7 — the rejected interface-union
/// method call leaks compiler internals: the `unknown` inference sentinel, the
/// `throws never` bottom marker (BAML has no `never` keyword), and the
/// method-as-value `(self: Cat) -> string` form. The message SHOULD name the real
/// receiver/method (or, per F7, the call should compile) — never these internals.
/// Repro: `cat_collection_union/collection_union_4_field_array.baml`
#[test]
fn union_fuzz_f12_iface_union_call_diagnostic_does_not_leak_internals() {
    let errors = collect_compile_errors(
        r#"
        interface Animal {
          function speak(self) -> string throws never
        }
        class Dog {
          implements Animal {
            function speak(self) -> string { return "Woof" }
          }
        }
        class Cat {
          implements Animal {
            function speak(self) -> string { return "Meow" }
          }
        }
        function describe(x: Animal | Cat) -> string {
          return x.speak()
        }
        function main() -> string {
          let a: Animal = Dog {}
          return describe(a)
        }
        "#,
    );
    assert!(
        errors
            .iter()
            .all(|e| !e.contains("throws never") && !e.contains("(self:")),
        "method-call diagnostic must not leak compiler internals \
         (`throws never`, `(self: ...)`, the `unknown` inference sentinel); got:\n  {}",
        errors.join("\n  ")
    );
}

/// F13 [bad-diagnostic]: the reserved internal `user.` package prefix leaks into
/// the E0062 non-exhaustive-match `missing:` class-pattern witnesses
/// (`user.Dog { owner: user.Person { ... } }`). User-facing output must never
/// contain `user.`.
/// Repro: `cat_match_union_exhaustive/match_union_exhaustive_11_nested_user_leak.baml`
#[test]
fn union_fuzz_f13_exhaustiveness_witness_does_not_leak_user_prefix() {
    let errors = collect_compile_errors(
        r#"
        class Person { name: string }
        class Dog { owner: Person }
        class Cat { lives: int }
        function describe(x: Dog | Cat) -> string {
          match (x) {
            Dog { owner: Person { name: "Alice" } } => "alice's dog"
          }
        }
        function main() -> string {
          let p: Person = Person { name: "Bob" }
          let d: Dog = Dog { owner: p }
          return describe(d)
        }
        "#,
    );
    // First require the E0062 non-exhaustive-match diagnostic to actually fire,
    // so the no-`user.` check below can't pass vacuously if exhaustiveness ever
    // stops being reported.
    assert!(
        errors.iter().any(|e| e.starts_with("[E0062]")),
        "expected an E0062 non-exhaustive-match error; got:\n  {}",
        errors.join("\n  ")
    );
    assert!(
        errors.iter().all(|e| !e.contains("user.")),
        "diagnostics must not leak the internal `user.` package prefix; got:\n  {}",
        errors.join("\n  ")
    );
}

/// F14 [bad-diagnostic]: the E0062 `missing:` witness renders interface union
/// members as a bare `_` (no `Ty::Interface` arm in the witness renderer), e.g.
/// `Animal | Vehicle` covered only by `Dog`/`Car` reports `missing: _, _`. The
/// non-exhaustiveness verdict is correct (interfaces are open-world), but the
/// witness SHOULD name the uncovered interface(s).
/// Repro: `cat_match_union_exhaustive/match_union_exhaustive_7_concrete_destructure_both_covers.baml`
#[test]
fn union_fuzz_f14_exhaustiveness_witness_names_interface_members() {
    let errors = collect_compile_errors(
        r#"
        interface Animal { function speak(self) -> string throws never }
        interface Vehicle { function drive(self) -> string throws never }
        class Dog { name: string  implements Animal { function speak(self) -> string { return "Woof" } } }
        class Car { model: string  implements Vehicle { function drive(self) -> string { return "Vroom" } } }
        function describe(x: Animal | Vehicle) -> string {
          match (x) {
            Dog { name } => "dog " + name
            Car { model } => "car " + model
          }
        }
        function main() -> string {
          let c: Car = Car { model: "T" }
          return describe(c)
        }
        "#,
    );
    assert!(
        errors.iter().any(|e| {
            e.starts_with("[E0062]")
                && e.split("missing:")
                    .nth(1)
                    .is_some_and(|w| w.contains("Animal") || w.contains("Vehicle"))
        }),
        "E0062 `missing:` witness should name the uncovered interface(s) (Animal/Vehicle), \
         not render them as bare `_`; got:\n  {}",
        errors.join("\n  ")
    );
}

/// F15 [bad-diagnostic]: an interface-union method-not-found wrongly blames a
/// member that DOES satisfy the interface — `x.debug()` on `int | Dog` emits an
/// E0007 against `int` even though `implements Debuggable for int` exists. It
/// should reject (Dog genuinely lacks `debug`) but blame ONLY `Dog`.
/// Repro: `cat_out_of_body_union/out_of_body_union_10_wrong_blame_int_with_impl.baml`
#[test]
fn union_fuzz_f15_union_method_blame_skips_satisfying_member() {
    let errors = collect_compile_errors(
        r#"
        interface Debuggable { function debug(self) -> string throws never }
        implements Debuggable for int { function debug(self) -> string { return "int" } }
        class Dog { name: string }
        function main() -> string {
          let x: int | Dog = 7
          return x.debug()
        }
        "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("Dog") && e.contains("debug")),
        "must still reject — `Dog` lacks `debug`; got:\n  {}",
        errors.join("\n  ")
    );
    assert!(
        !errors
            .iter()
            .any(|e| e.contains("`int`") && e.contains("debug")),
        "must NOT blame `int` — it satisfies Debuggable via `implements Debuggable for int`; \
         got:\n  {}",
        errors.join("\n  ")
    );
}

/// F16 [bad-diagnostic]: an unqualified ambiguous interface-field access on a
/// UNION value gives a false E0007 `no member` instead of the E0131 ambiguity
/// diagnostic (with the `.as<Named>`/`.as<Labeled>` hint) that the equivalent
/// single-class case already emits.
/// Repro: `cat_as_projection_union/as_projection_union_22_unqualified_ambiguous.baml`
#[test]
fn union_fuzz_f16_unqualified_ambiguous_union_field_is_e0131() {
    assert_compile_error_code(
        r#"
        interface Named { name: string }
        interface Labeled { name: string }
        class Person {
          full: string
          handle: string
          implements Named { name as full }
          implements Labeled { name as handle }
        }
        class Company {
          legal: string
          brand: string
          implements Named { name as legal }
          implements Labeled { name as brand }
        }
        function rawName(x: Person | Company) -> string {
          return x.name
        }
        function main() -> string {
          return rawName(Person { full: "Ada", handle: "ada99" })
        }
        "#,
        "E0131",
    );
}

#[test]
fn conjunction_ambiguous_source_fields_keep_field_diagnostic() {
    assert_compile_error_contains(
        r#"
        interface Left { value: string }
        interface Right { value: int }
        function read<T extends Left & Right>(value: T) -> string {
            return value.value
        }
        "#,
        "field `value` on class",
    );
}

#[test]
fn conjunction_ambiguous_source_fields_diagnostic_names_ambiguity() {
    // The assertion above pins the field; this pins the ambiguity wording so a
    // different `value`-mentioning diagnostic (unknown field, bound failure)
    // cannot satisfy the suite (PR #4332 review).
    assert_compile_error_contains(
        r#"
        interface Left { value: string }
        interface Right { value: int }
        function read<T extends Left & Right>(value: T) -> string {
            return value.value
        }
        "#,
        "is ambiguous because it is declared by multiple interfaces",
    );
}

/// F17 [bad-diagnostic]: a failed `.as<Cargo<int>>` projection drops the `<int>`
/// type argument from the message (`does not implement interface Cargo`), which
/// is misleading because the type DOES implement `Cargo`, just at `<string>`.
/// The diagnostic SHOULD name the full `Cargo<int>`.
/// Repro: `cat_as_projection_union/as_projection_union_10_generic_mismatch.baml`
#[test]
fn union_fuzz_f17_projection_failure_names_full_generic_interface() {
    assert_compile_error_contains(
        r#"
        interface Cargo<T> { payload: T }
        class Box<T> {
          inner: T
          implements Cargo<T> { payload as inner }
        }
        function takeInt(x: Box<int> | Box<string>) -> int {
          return x.as<Cargo<int>>.payload
        }
        function main() -> int {
          let b: Box<int> | Box<string> = Box<string> { inner: "oops" }
          return takeInt(b)
        }
        "#,
        "Cargo<int>",
    );
}

// ── Group R: dispatch-guard and call-site type-arg regressions ───────────────
// Pinned while implementing BEP-060 (baml.csv), whose iterators were the
// first stdlib classes to exercise these paths.

/// Explicit call-site type args on an abstract interface method reached
/// through a field chain still go through the declared-generic-params lookup:
/// wrong arity is a compile error, not silently dropped type args.
#[test]
fn field_chain_abstract_interface_method_arity_checked() {
    assert_compile_error_contains(
        r#"
        interface Conv {
            function convert<T>(self, v: T) -> T throws never
        }
        class Celsius {
            tag: string
            implements Conv {
                function convert<T>(self, v: T) -> T { v }
            }
        }
        class Holder {
            c: Conv
        }
        function main() -> int {
            let h = Holder { c: Celsius { tag: "x" } };
            h.c.convert<int, string>(5)
        }
    "#,
        "type argument",
    );
}

/// Two `implements` blocks of the same interface on one class that differ
/// ONLY in their associated-type bindings are rejected, exactly like fully
/// identical duplicates. The MIR dispatch-guard wildcard for unpinnable
/// typevar-union bindings (`type Error = E1 | E2` matched against a
/// normalized request) leans on this invariant: same-class arms can only
/// coexist when their positional interface args differ, so leaving assoc
/// bindings unpinned never has to discriminate between two arms.
#[test]
fn duplicate_implements_differing_only_in_assoc_bindings_is_compile_error() {
    assert_compile_error_code(
        r#"
        interface Sink {
            type Error
            function push(self, v: int) -> string throws never
        }

        class Buf<E1, E2> {
            tag: string
            implements Sink {
                type Error = E1
                function push(self, v: int) -> string { "a" }
            }
            implements Sink {
                type Error = E2
                function push(self, v: int) -> string { "b" }
            }
        }
        "#,
        "E0132",
    );
}

// ── Group AH: comparison operators require a single concrete `Compare` type ──
// Ordering (`<` `<=` `>` `>=`) is valid only when both operands are the *same
// concrete type* implementing `baml.ops.Compare` (or the same bounded type-var).
// A union, an interface-existential, or two different types is a compile error.
//
// That restriction is load-bearing, not merely conservative: a valid ordering over
// a non-primitive lowers to a `baml.ops.Compare` virtual call resolved from the
// *receiver's* concrete type alone (`lower_binary`). Single dispatch is only sound
// because both operands are guaranteed to be the same concrete type at runtime.

#[test]
fn ordering_on_union_operands_is_rejected() {
    // `int | string` is member-wise a `Compare` subtype, but it is not a single
    // concrete type — the two operands could hold different concretes, which the
    // VM cannot order. (`f(5, "a")` would otherwise abort at runtime.)
    assert_compile_error_contains(
        r#"
        function f(a: int | string, b: int | string) -> bool {
            a < b
        }
        "#,
        "does not implement `Compare`",
    );
}

#[test]
fn ordering_on_different_concrete_types_is_rejected() {
    assert_compile_error_contains(
        r#"
        function f(a: int, b: string) -> bool {
            a < b
        }
        "#,
        "ordering requires both operands",
    );
}

#[test]
fn ordering_on_interface_existential_is_rejected() {
    // Two `baml.ops.Compare` existentials could be different concrete types, so
    // ordering them is not exact-type and is rejected.
    assert_compile_error_contains(
        r#"
        function f(a: baml.ops.Compare, b: baml.ops.Compare) -> bool {
            a < b
        }
        "#,
        "does not implement `Compare`",
    );
}

#[test]
fn ordering_diagnostic_renders_operator_symbol() {
    // The diagnostic prints the operator *symbol* (`<`), not its Debug name (`Lt`).
    assert_compile_error_contains(
        r#"
        function f(a: int, b: string) -> bool {
            a < b
        }
        "#,
        "with `<`",
    );
}

#[test]
fn ordering_on_same_concrete_primitive_is_ok() {
    // Guards against over-rejection: `int < int` is the canonical valid case.
    assert_no_compile_errors(
        r#"
        function f(a: int, b: int) -> bool {
            a < b
        }
        "#,
    );
}

// (The `int | 99` catch-result ordering — where the union must be normalized to
// its single concrete base `int` before the union rejection — is exercised at
// runtime by `baml_src/ns_arrays/sort_comparable.baml`.)

// The tests below are TIR-only: these helpers collect diagnostics and never run
// MIR lowering, so they pin the *accepted set* rather than the dispatch. They are
// the guard that the shapes `lower_ordering_via_virtual_call` exists to serve stay
// accepted, and that the shapes its soundness depends on stay rejected. Runtime
// behavior of the lowering lives in `bex_vm/tests/comparison_driver.rs` and
// `baml_src/ns_operators/operators.baml`.

#[test]
fn ordering_on_user_class_implementing_compare_is_ok() {
    // Guards against over-rejection: a class implementing `Compare` may be
    // ordered. Only the required `lt` is defined — `<=`/`>`/`>=` reach the
    // interface's defaults.
    assert_no_compile_errors(
        r#"
        class Money {
            cents: int
            implements baml.ops.Equals {
                function eq(self, other: Self) -> bool throws never { self.cents == other.cents }
            }
            implements baml.ops.Compare {
                function lt(self, other: Self) -> bool throws never { self.cents < other.cents }
            }
        }
        function f(a: Money, b: Money) -> bool throws never {
            (a < b) && (a <= b) && (a > b) && (a >= b)
        }
        "#,
    );
}

#[test]
fn ordering_on_bounded_type_var_is_ok() {
    // `T extends Compare` is a single concrete type per instantiation, so ordering
    // is exact-type. The impl can only come from the runtime instantiation.
    assert_no_compile_errors(
        r#"
        function max<T extends baml.ops.Compare>(a: T, b: T) -> T throws never {
            if a < b { b } else { a }
        }
        "#,
    );
}

#[test]
fn compare_bound_rejects_abstract_type_argument() {
    // The counterpart to `ordering_on_union_operands_is_rejected`: a union cannot
    // sneak in through a type argument either. This is what makes the operand of a
    // `Compare`-bounded ordering a single concrete type at runtime, and hence what
    // makes single-dispatch (`Compare.lt` resolved on the receiver alone, in
    // `lower_ordering_via_virtual_call`) sound.
    assert_compile_error_code(
        r#"
        function max<T extends baml.ops.Compare>(a: T, b: T) -> T throws never {
            if a < b { b } else { a }
        }
        function f(a: int | string, b: int | string) -> int | string throws never {
            max<int | string>(a, b)
        }
        "#,
        "E0001",
    );
}

#[test]
fn ordering_on_interface_existential_type_argument_is_rejected() {
    // Same premise via the other abstract spelling: an interface-existential type
    // argument has no single runtime type to dispatch on either.
    assert_compile_error_code(
        r#"
        function max<T extends baml.ops.Compare>(a: T, b: T) -> T throws never {
            if a < b { b } else { a }
        }
        function f(a: baml.ops.Compare, b: baml.ops.Compare) -> baml.ops.Compare throws never {
            max<baml.ops.Compare>(a, b)
        }
        "#,
        "E0001",
    );
}

#[test]
fn compare_without_equals_is_rejected() {
    // `Compare requires Equals`, and the inherited `le` default is literally
    // `self.lt(other) || self.eq(other)`. If a type could implement `Compare`
    // without `Equals`, `a <= b` would lower to a virtual call whose `eq` has no
    // impl to resolve — an uncatchable internal error. E0125 is what prevents it.
    assert_compile_error_code(
        r#"
        class NoEq {
            v: int
            implements baml.ops.Compare {
                function lt(self, other: Self) -> bool throws never { self.v < other.v }
            }
        }
        "#,
        "E0125",
    );
}

// ── Group AI: arithmetic operators dispatch through the `baml.ops` interfaces ──
// `+ - * / %` (and unary `-`) are valid iff the operand types implement the
// matching `baml.ops` interface for the right operand; the result is the impl's
// `Output`. Unions are valid iff every operand pair is, with the union of their
// outputs.

#[test]
fn scalar_arithmetic_matches_builtin_impl_matrix() {
    let types = [
        ("int", "int"),
        ("float", "float"),
        ("bigint", "bigint"),
        ("string", "string"),
        ("bool", "bool"),
        ("null", "null"),
    ];
    let operators = [
        ("add", "+"),
        ("sub", "-"),
        ("mul", "*"),
        ("div", "/"),
        ("rem", "%"),
    ];
    let numeric_pair = |lhs: &str, rhs: &str| {
        matches!(
            (lhs, rhs),
            ("int", "int")
                | ("int", "float")
                | ("float", "int")
                | ("float", "float")
                | ("int", "bigint")
                | ("bigint", "int")
                | ("bigint", "bigint")
        )
    };

    let mut valid_source = String::new();
    let mut invalid_source = String::new();
    let mut invalid_count = 0;
    for (lhs_name, lhs_ty) in types {
        for (rhs_name, rhs_ty) in types {
            for (op_name, symbol) in operators {
                let is_valid = numeric_pair(lhs_name, rhs_name)
                    || (lhs_name == "string" && rhs_name == "string" && op_name == "add");
                let source = format!(
                    "function {op_name}_{lhs_name}_{rhs_name}(lhs: {lhs_ty}, rhs: {rhs_ty}) -> int {{ lhs {symbol} rhs; 0 }}\n"
                );
                if is_valid {
                    valid_source.push_str(&source);
                } else {
                    invalid_source.push_str(&source);
                    invalid_count += 1;
                }
            }
        }
    }

    assert_no_compile_errors(&valid_source);
    let errors = collect_compile_errors(&invalid_source);
    assert_eq!(
        errors.len(),
        invalid_count,
        "each missing impl must produce one error:\n  {}",
        errors.join("\n  ")
    );
    assert!(
        errors
            .iter()
            .all(|error| error.contains("cannot be applied")),
        "unexpected diagnostics:\n  {}",
        errors.join("\n  ")
    );
}

#[test]
fn structural_arithmetic_without_impls_is_rejected() {
    let errors = collect_compile_errors(
        r#"
        function f(xs: int[], values: map<string, int>, n: float) -> int {
            xs + n;
            n + xs;
            values + n;
            n + values;
            0
        }
        "#,
    );
    assert_eq!(errors.len(), 4, "unexpected diagnostics: {errors:#?}");
    assert!(
        errors
            .iter()
            .all(|error| error.contains("cannot be applied"))
    );
}

#[test]
fn arithmetic_on_user_type_implementing_add_is_ok() {
    assert_no_compile_errors(
        r#"
        class Vec2 {
            x: int
            y: int
            implements baml.ops.Add<Vec2> {
                type Output = Vec2
                function add(self, rhs: Vec2) -> Vec2 throws never {
                    Vec2 { x: self.x + rhs.x, y: self.y + rhs.y }
                }
            }
        }
        function f(a: Vec2, b: Vec2) -> Vec2 {
            a + b
        }
        "#,
    );
}

#[test]
fn arithmetic_on_user_type_without_impl_is_rejected() {
    assert_compile_error_contains(
        r#"
        class Vec2 { x: int }
        function f(a: Vec2, b: Vec2) -> Vec2 {
            a + b
        }
        "#,
        "cannot be applied",
    );
}

#[test]
fn arithmetic_output_type_is_the_impl_output() {
    // `Add<int> for Counter` has `Output = int`, so `c + 1` is an `int`.
    assert_no_compile_errors(
        r#"
        class Counter {
            n: int
            implements baml.ops.Add<int> {
                type Output = int
                function add(self, rhs: int) -> int throws never { self.n + rhs }
            }
        }
        function f(c: Counter) -> int {
            c + 1
        }
        "#,
    );
}

#[test]
fn negate_on_user_type_implementing_negate_is_ok() {
    assert_no_compile_errors(
        r#"
        class Vec2 {
            x: int
            implements baml.ops.Negate {
                type Output = Vec2
                function neg(self) -> Vec2 throws never { Vec2 { x: -self.x } }
            }
        }
        function f(a: Vec2) -> Vec2 {
            -a
        }
        "#,
    );
}

#[test]
fn negate_on_user_type_without_impl_is_rejected() {
    assert_compile_error_contains(
        r#"
        class Vec2 { x: int }
        function f(a: Vec2) -> Vec2 {
            -a
        }
        "#,
        "cannot be applied",
    );
}

#[test]
fn arithmetic_on_union_all_pairs_valid_is_ok() {
    // `int | bigint` + `int`: int+int and bigint+int both implement Add, so the
    // result is `int | bigint`.
    assert_no_compile_errors(
        r#"
        function f(a: int | bigint, b: int) -> int | bigint {
            a + b
        }
        "#,
    );
}

#[test]
fn arithmetic_out_of_body_user_impl_is_ok() {
    assert_no_compile_errors(
        r#"
        class Counter { n: int }
        implement baml.ops.Add<int> for Counter {
            type Output = int
            function add(self, rhs: int) -> int throws never { self.n + rhs }
        }
        function f(c: Counter) -> int { c + 1 }
        "#,
    );
}

#[test]
fn arithmetic_on_interface_existential_is_ok() {
    // An interface-existential operand (all associated types specified) dispatches
    // through its pinned `Output`: `Add<int, Output=int> + int` is an `int`.
    assert_no_compile_errors(
        r#"
        function f(x: baml.ops.Add<int, Output = int>, a: int) -> int {
            x + a
        }
        "#,
    );
}

#[test]
fn arithmetic_on_userclass_interface_existential_is_ok() {
    // A user class as the interface arg must satisfy `Rhs extends Concrete` via
    // the stdlib's blanket `Concrete` impl (which lives in the baml package, not
    // the user's) — the bound is an implements check across both packages.
    assert_no_compile_errors(
        r#"
        class B { v: int }
        class A {
            v: int
            implements baml.ops.Add<A> {
                type Output = B
                function add(self, rhs: A) -> B throws never { B { v: self.v + rhs.v } }
            }
        }
        function f(x: baml.ops.Add<A, Output = B>, a: A) -> B {
            x + a
        }
        "#,
    );
}

#[test]
fn arithmetic_on_mixed_primitive_union_uses_cartesian_product() {
    // `(int | float) + int` used to promote to `float` on the primitive fast
    // path, leaving a runtime `int` in a float-typed slot (UB in the
    // specialized opcodes). The interface path types it as the union of the
    // pair Outputs: `int | float`.
    assert_no_compile_errors(
        r#"
        function pick(n: int) -> int | float {
            if n > 0 { 1 } else { 2.5 }
        }
        function f(n: int) -> int | float {
            pick(n) + 1
        }
        "#,
    );
}

#[test]
fn arithmetic_on_existential_without_pinned_output_is_rejected() {
    // Without an `Output` pin the `= Self` default realizes to the existential
    // itself — an unsound claim (`Output` is only bound by `Concrete`), so the
    // operand must specify `Output`.
    assert_compile_error_contains(
        r#"
        function f(x: baml.ops.Add<int>, a: int) -> int {
            x + a
        }
        "#,
        "cannot be applied",
    );
}

#[test]
fn arithmetic_on_bounded_typevar_with_pinned_output_is_ok() {
    assert_no_compile_errors(
        r#"
        function f<T extends baml.ops.Add<int, Output = int>>(x: T) -> int {
            x + 1
        }
        "#,
    );
}

#[test]
fn negate_output_type_is_the_impl_output() {
    // `Negate` has an `Output` (defaulting to `Self`), so `-d` can change type.
    assert_no_compile_errors(
        r#"
        class Debt {
            amount: int
            implements baml.ops.Negate {
                type Output = int
                function neg(self) -> int throws never { 0 - self.amount }
            }
        }
        function f(d: Debt) -> int {
            -d
        }
        "#,
    );
}

#[test]
fn negate_on_bounded_typevar_without_pinned_output_is_rejected() {
    // Same rule as the binary operators: an operand whose `Output` realizes to
    // the unpinned `= Self` default existential is invalid.
    assert_compile_error_contains(
        r#"
        function f<T extends baml.ops.Negate>(x: T) -> int {
            -x;
            0
        }
        "#,
        "cannot be applied",
    );
}

#[test]
fn arithmetic_on_bounded_typevar_without_pinned_output_is_rejected() {
    assert_compile_error_contains(
        r#"
        function f<T extends baml.ops.Add<int>>(x: T) -> int {
            x + 1;
            0
        }
        "#,
        "cannot be applied",
    );
}

#[test]
fn compound_assign_result_not_assignable_to_target_is_rejected() {
    // `c += 1` desugars to `c = c + 1`; with `Output = int` the operator result
    // is an `int`, which cannot be stored back into the `Counter` target.
    assert_compile_error_contains(
        r#"
        class Counter {
            n: int
            implements baml.ops.Add<int> {
                type Output = int
                function add(self, rhs: int) -> int throws never { self.n + rhs }
            }
        }
        function f(c: Counter) -> Counter {
            c += 1;
            c
        }
        "#,
        "mismatched types",
    );
}

#[test]
fn compound_assign_on_user_type_with_self_output_is_ok() {
    assert_no_compile_errors(
        r#"
        class Vec2 {
            x: int
            implements baml.ops.Add<Vec2> {
                type Output = Vec2
                function add(self, rhs: Vec2) -> Vec2 throws never {
                    Vec2 { x: self.x + rhs.x }
                }
            }
        }
        function f(v: Vec2, w: Vec2) -> Vec2 {
            v += w;
            v
        }
        "#,
    );
}
