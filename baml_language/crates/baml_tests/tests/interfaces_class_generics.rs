//! Focused tests for interface-related behavior on class generic parameters.
//!
//! These tests live separately from the large interface suite so regressions in
//! class-specific generic-bound behavior are easy to diagnose.

use std::collections::HashSet;

use baml_compiler_diagnostics::Severity;
use baml_db::{collect_diagnostics, testing::setup_test_db};

fn collect_compile_errors(source: &str) -> Vec<String> {
    let db = setup_test_db(source);
    let all_files = db.workspace_files();
    let user_file_ids: HashSet<_> = all_files.iter().map(|f| f.file_id(&db)).collect();

    collect_diagnostics(&db)
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
fn assert_compile_error_contains(source: &str, needle: &str) {
    let errors = collect_compile_errors(source);
    assert!(
        errors.iter().any(|e| e.contains(needle)),
        "expected a compile error containing {needle:?}; got:\n  {}",
        errors.join("\n  ")
    );
}

#[track_caller]
fn assert_compile_error_contains_any(source: &str, needles: &[&str]) {
    let errors = collect_compile_errors(source);
    assert!(
        needles
            .iter()
            .any(|needle| errors.iter().any(|e| e.contains(needle))),
        "expected a compile error containing one of {needles:?}; got:\n  {}",
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

#[test]
fn class_generic_bound_exposes_interface_members() {
    assert_no_compile_errors(
        r#"
        interface Named {
            name: string
        }

        class Box<T extends Named> {
            value: T

            function get_name(self) -> string {
                return self.value.name
            }
        }
        "#,
    );
}

#[test]
fn class_generic_bound_accepts_implementing_class() {
    assert_no_compile_errors(
        r#"
        interface Named {
            name: string
        }

        class Dog {
            name: string
            implements Named {}
        }

        class Box<T extends Named> {
            value: T
        }

        function main() -> Box<Dog> {
            return Box<Dog> { value: Dog { name: "Rex" } }
        }
        "#,
    );
}

#[test]
fn class_generic_bound_violation_is_compile_error_for_explicit_arg() {
    assert_compile_error_contains(
        r#"
        interface Named {
            name: string
        }

        class Box<T extends Named> {
            value: T
        }

        function main() -> Box<int> {
            return Box<int> { value: 1 }
        }
        "#,
        "Named",
    );
}

#[test]
fn class_generic_bound_violation_is_compile_error_for_inferred_arg() {
    assert_compile_error_contains(
        r#"
        interface Named {
            name: string
        }

        class Box<T extends Named> {
            value: T
        }

        function main() -> int {
            let bad = Box { value: 1 }
            return 1
        }
        "#,
        "Named",
    );
}

#[test]
fn class_generic_bound_through_requires_chain_exposes_all_members() {
    assert_no_compile_errors(
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
            implements Named {}
            implements Person {}
        }

        class Holder<T extends Person> {
            value: T

            function label(self) -> string {
                return self.value.name + ":" + self.value.occupation
            }
        }

        function main() -> Holder<Employee> {
            return Holder<Employee> {
                value: Employee { name: "Ada", occupation: "Engineer" }
            }
        }
        "#,
    );
}

#[test]
fn class_generic_bound_is_visible_inside_lambda_body() {
    assert_no_compile_errors(
        r#"
        interface Named {
            name: string
        }

        class Dog {
            name: string
            implements Named {}
        }

        class Box<T extends Named> {
            value: T

            function get_name(self) -> string {
                let read = () -> string {
                    return self.value.name
                }
                return read()
            }
        }

        function main() -> string {
            return Box<Dog> { value: Dog { name: "Rex" } }.get_name()
        }
        "#,
    );
}

#[test]
fn class_generic_bound_rejects_union_bound() {
    // Bounds are interfaces only — a union type is not an interface (there is no
    // `implements` relation to a union), so it cannot be a generic bound.
    assert_compile_error_contains(
        r#"
        class Box<T extends int | string> {
            value: T
        }

        function main() -> int {
            return 1
        }
        "#,
        "is not an interface",
    );
}

#[test]
fn class_generic_bound_rejects_type_outside_union() {
    assert_compile_error_contains_any(
        r#"
        class Box<T extends int | string> {
            value: T
        }

        function main() -> Box<bool> {
            return Box<bool> { value: true }
        }
        "#,
        &["int | string", "type mismatch"],
    );
}

#[test]
fn class_generic_bound_rejects_container_and_optional_bounds() {
    // Bounds are interfaces only — a list or optional type is not an interface.
    assert_compile_error_contains(
        r#"
        class ListBox<T extends int[]> {
            value: T
        }

        function main() -> int {
            return 1
        }
        "#,
        "is not an interface",
    );
}

#[test]
fn class_generic_bound_rejects_wrong_compound_type() {
    assert_compile_error_contains_any(
        r#"
        class ListBox<T extends int[]> {
            value: T
        }

        function main() -> ListBox<string[]> {
            return ListBox<string[]> { value: ["nope"] }
        }
        "#,
        &["int[]", "type mismatch"],
    );
}

#[test]
fn class_generic_bound_rejects_function_type_bound() {
    // Bounds are interfaces only — a function type is not an interface.
    assert_compile_error_contains(
        r#"
        interface Named {
            name: string
        }

        class FunctionBox<T extends () -> Named> {
            cb: T
        }

        function main() -> int {
            return 1
        }
        "#,
        "is not an interface",
    );
}

#[test]
fn class_generic_bound_rejects_interface_implementors_inside_invariant_containers() {
    assert_compile_error_contains_any(
        r#"
        interface Named {
            name: string
        }

        class Dog {
            name: string
            implements Named {}
        }

        class ListBox<T extends Named[]> {
            value: T
        }

        class MapBox<T extends map<string, Named>> {
            value: T
        }

        function main() -> int {
            let xs = ListBox<Dog[]> { value: [Dog { name: "Rex" }] }
            let directory = MapBox<map<string, Dog>> { value: { "rex": Dog { name: "Rex" } } }
            return 1
        }
        "#,
        &["Named[]", "map<string, Named>", "type mismatch"],
    );
}

#[test]
fn class_generic_bound_rejects_nested_interface_bound_violation() {
    assert_compile_error_contains(
        r#"
        interface Named {
            name: string
        }

        class ListBox<T extends Named[]> {
            value: T
        }

        function main() -> ListBox<int[]> {
            return ListBox<int[]> { value: [1] }
        }
        "#,
        "Named",
    );
}

#[test]
fn class_generic_bound_substitutes_other_class_type_params() {
    assert_no_compile_errors(
        r#"
        interface Slot<T> {
            value: T
        }

        class IntSlot {
            value: int
            implements Slot<int> {}
        }

        class Holder<T extends Slot<U>, U> {
            item: T

            function read(self) -> U {
                return self.item.value
            }
        }

        function main() -> Holder<IntSlot, int> {
            return Holder<IntSlot, int> { item: IntSlot { value: 1 } }
        }
        "#,
    );
}

#[test]
fn class_generic_bound_rejects_after_substituting_other_type_params() {
    assert_compile_error_contains_any(
        r#"
        interface Slot<T> {
            value: T
        }

        class IntSlot {
            value: int
            implements Slot<int> {}
        }

        class Holder<T extends Slot<U>, U> {
            item: T
        }

        function main() -> Holder<IntSlot, string> {
            return Holder<IntSlot, string> { item: IntSlot { value: 1 } }
        }
        "#,
        &["Slot<string>", "type mismatch"],
    );
}

#[test]
fn class_generic_bound_rejects_function_type_argument_bound() {
    // Bounds are interfaces only — a function type (even a concrete one) is not an
    // interface, so it cannot be a generic bound.
    assert_compile_error_contains(
        r#"
        class CallbackBox<T extends (x: int) -> int> {
            cb: T
        }

        function main() -> int {
            return 1
        }
        "#,
        "is not an interface",
    );
}

#[test]
fn class_generic_bound_rejects_non_matching_function_type_arguments() {
    assert_compile_error_contains_any(
        r#"
        type StringCallback = (x: string) -> string

        class CallbackBox<T extends (x: int) -> int> {
            cb: T
        }

        function echo(x: string) -> string {
            return x
        }

        function main() -> CallbackBox<StringCallback> {
            return CallbackBox<StringCallback> { cb: echo }
        }
        "#,
        &["(x: int) -> int", "type mismatch"],
    );
}

#[test]
fn class_generic_bound_is_enforced_in_parameter_annotations() {
    assert_compile_error_contains(
        r#"
        interface Named {
            name: string
        }

        class Box<T extends Named> {
            value: T
        }

        function takes_bad(x: Box<int>) -> int {
            return 1
        }
        "#,
        "Named",
    );
}

#[test]
fn class_generic_bound_is_enforced_in_class_field_annotations() {
    assert_compile_error_contains(
        r#"
        interface Named {
            name: string
        }

        class Box<T extends Named> {
            value: T
        }

        class Bad {
            value: Box<int>
        }
        "#,
        "Named",
    );
}

#[test]
fn class_generic_bound_is_enforced_in_type_aliases() {
    assert_compile_error_contains(
        r#"
        interface Named {
            name: string
        }

        class Box<T extends Named> {
            value: T
        }

        type Bad = Box<int>
        "#,
        "Named",
    );
}

#[test]
fn class_generic_bound_is_enforced_in_optional_type_aliases() {
    assert_compile_error_contains(
        r#"
        interface Named {
            name: string
        }

        class Box<T extends Named> {
            value: T
        }

        type Bad = Box<int>?
        "#,
        "Named",
    );
}

#[test]
fn class_generic_bound_is_enforced_when_alias_is_used() {
    assert_compile_error_contains(
        r#"
        interface Named {
            name: string
        }

        class Box<T extends Named> {
            value: T
        }

        type Bad = Box<int>?

        function takes_bad(value: Bad) -> int {
            return 1
        }
        "#,
        "Named",
    );
}

#[test]
fn class_generic_bound_is_enforced_in_optional_annotations() {
    assert_compile_error_contains(
        r#"
        interface Named {
            name: string
        }

        class Box<T extends Named> {
            value: T
        }

        function takes_bad(value: Box<int>?) -> int {
            return 1
        }
        "#,
        "Named",
    );
}

#[test]
fn class_generic_bound_allows_bounded_type_var_as_type_arg_in_class_field() {
    assert_no_compile_errors(
        r#"
        interface Marker {}

        class Box<T extends Marker> {
            value: T
        }

        class Outer<T extends Marker> {
            inner: Box<T>
        }

        function main() -> int {
            return 1
        }
        "#,
    );
}

#[test]
fn class_generic_bound_is_enforced_in_function_throws_annotations() {
    assert_compile_error_contains(
        r#"
        interface Named {
            name: string
        }

        class Box<T extends Named> {
            value: T
        }

        function bad() -> int throws Box<int> {
            return 1
        }
        "#,
        "Named",
    );
}

#[test]
fn class_generic_bound_is_enforced_in_lambda_annotations() {
    assert_compile_error_contains(
        r#"
        interface Named {
            name: string
        }

        class Box<T extends Named> {
            value: T
        }

        function main() -> int {
            let f = () -> Box<int> {
                return Box<int> { value: 1 }
            }
            return 1
        }
        "#,
        "Named",
    );
}

#[test]
fn class_generic_bound_is_enforced_in_lambda_throws_annotations() {
    assert_compile_error_contains(
        r#"
        interface Named {
            name: string
        }

        class Box<T extends Named> {
            value: T
        }

        function main() -> int {
            let f = () -> int throws Box<int> {
                return 1
            }
            return 1
        }
        "#,
        "Named",
    );
}

#[test]
fn class_generic_bound_is_enforced_in_let_else_patterns() {
    assert_compile_error_contains(
        r#"
        interface Named {
            name: string
        }

        class Box<T extends Named> {
            value: T
        }

        function main(x: Box<Named>?) -> int {
            let b: Box<int> = x else {
                return 0
            }
            return 1
        }
        "#,
        "Named",
    );
}

#[test]
fn class_generic_bound_is_enforced_in_match_patterns() {
    assert_compile_error_contains(
        r#"
        interface Named {
            name: string
        }

        class Box<T extends Named> {
            value: T
        }

        function main(x: Box<Named> | int) -> int {
            return match (x) {
                let b: Box<int> => 1,
                _ => 0
            }
        }
        "#,
        "Named",
    );
}

#[test]
fn class_generic_bound_is_enforced_in_interface_field_annotations() {
    assert_compile_error_contains(
        r#"
        interface Named {
            name: string
        }

        class Box<T extends Named> {
            value: T
        }

        interface Bad {
            value: Box<int>
        }
        "#,
        "Named",
    );
}

#[test]
fn class_generic_bound_is_enforced_in_required_interface_method_annotations() {
    assert_compile_error_contains(
        r#"
        interface Named {
            name: string
        }

        class Box<T extends Named> {
            value: T
        }

        interface Bad {
            function f(self, value: Box<int>) -> Box<int> throws never
        }
        "#,
        "Named",
    );
}

// A method's own generic bound may reference the enclosing class param
// (`<U extends Eq<C>>` on a method of `class Wrapper<C>`). On a bound-method
// call the value of `C` is supplied by the receiver, so the bound must be
// resolved against the receiver's class type arg before it is checked.
#[test]
fn method_generic_bound_referencing_class_param_is_satisfied() {
    assert_no_compile_errors(
        r#"
        interface Eq<T> { function equal(self, other: T) -> bool throws never }
        class Wrapper<C> {
            inner: C
            function compare_with<U extends Eq<C>>(self, u: U) -> bool { return true }
        }
        class IntEq {
            implements Eq<int> { function equal(self, other: int) -> bool { return true } }
        }
        function main() -> bool {
            let w = Wrapper<int> { inner: 5 }
            return w.compare_with<IntEq>(IntEq {})
        }
        "#,
    );
}

#[test]
fn method_generic_bound_referencing_class_param_is_enforced() {
    // The same call with an argument that satisfies `Eq<string>` (not `Eq<int>`,
    // the receiver's specialization) must be rejected — the bound resolves to
    // `Eq<int>` via the receiver, so `StrEq` does not satisfy it.
    assert_compile_error_contains(
        r#"
        interface Eq<T> { function equal(self, other: T) -> bool throws never }
        class Wrapper<C> {
            inner: C
            function compare_with<U extends Eq<C>>(self, u: U) -> bool { return true }
        }
        class StrEq {
            implements Eq<string> { function equal(self, other: string) -> bool { return true } }
        }
        function main() -> bool {
            let w = Wrapper<int> { inner: 5 }
            return w.compare_with<StrEq>(StrEq {})
        }
        "#,
        "Eq<int>",
    );
}
