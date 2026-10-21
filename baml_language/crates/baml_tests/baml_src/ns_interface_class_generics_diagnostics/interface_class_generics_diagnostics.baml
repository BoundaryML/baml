// Reflected compiler diagnostics migrated from the Rust interface suite.
// Every fixture is isolated so intentionally repeated declarations cannot collide.

function ErrorDiagnostics(files: map<string, string>) -> reflect.Diagnostic[] throws never {
    root.compiler.CompileDiagnostics(files).filter((diagnostic) -> bool {
        diagnostic.severity == "error"
    })
}

function AssertHasAnyMessage(files: map<string, string>, needles: string[]) -> void throws never {
    let diagnostics = ErrorDiagnostics(files);
    for (let needle in needles) {
        for (let diagnostic in diagnostics) {
            if (diagnostic.message.includes(needle)) {
                return;
            }
        }
    }
    baml.sys.panic(`expected compiler diagnostic containing one of ${needles}`)
}

test "class_generic_bound_violation_is_compile_error_for_explicit_arg" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_violation_is_compile_error_for_explicit_arg.baml": `
                interface Named {
                    name: string
                }

                class Box<T extends Named> {
                    value: T
                }

                function main() -> Box<int> {
                    return Box<int> { value: 1 }
                }
            `,
        },
        "Named",
    )
}

test "class_generic_bound_violation_is_compile_error_for_inferred_arg" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_violation_is_compile_error_for_inferred_arg.baml": `
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
            `,
        },
        "Named",
    )
}

test "class_generic_bound_rejects_union_bound" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_rejects_union_bound.baml": `
                class Box<T extends int | string> {
                    value: T
                }

                function main() -> int {
                    return 1
                }
            `,
        },
        "is not an interface",
    )
}

test "class_generic_bound_rejects_type_outside_union" {
    AssertHasAnyMessage(
        {
            "class_generic_bound_rejects_type_outside_union.baml": `
                class Box<T extends int | string> {
                    value: T
                }

                function main() -> Box<bool> {
                    return Box<bool> { value: true }
                }
            `,
        },
        ["int | string", "type mismatch"],
    )
}

test "class_generic_bound_rejects_container_and_optional_bounds" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_rejects_container_and_optional_bounds.baml": `
                class ListBox<T extends int[]> {
                    value: T
                }

                function main() -> int {
                    return 1
                }
            `,
        },
        "is not an interface",
    )
}

test "class_generic_bound_rejects_wrong_compound_type" {
    AssertHasAnyMessage(
        {
            "class_generic_bound_rejects_wrong_compound_type.baml": `
                class ListBox<T extends int[]> {
                    value: T
                }

                function main() -> ListBox<string[]> {
                    return ListBox<string[]> { value: ["nope"] }
                }
            `,
        },
        ["int[]", "type mismatch"],
    )
}

test "class_generic_bound_rejects_function_type_bound" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_rejects_function_type_bound.baml": `
                interface Named {
                    name: string
                }

                class FunctionBox<T extends () -> Named> {
                    cb: T
                }

                function main() -> int {
                    return 1
                }
            `,
        },
        "is not an interface",
    )
}

test "class_generic_bound_rejects_interface_implementors_inside_invariant_containers" {
    AssertHasAnyMessage(
        {
            "class_generic_bound_rejects_interface_implementors_inside_invariant_containers.baml": `
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
            `,
        },
        ["Named[]", "map<string, Named>", "type mismatch"],
    )
}

test "class_generic_bound_rejects_nested_interface_bound_violation" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_rejects_nested_interface_bound_violation.baml": `
                interface Named {
                    name: string
                }

                class ListBox<T extends Named[]> {
                    value: T
                }

                function main() -> ListBox<int[]> {
                    return ListBox<int[]> { value: [1] }
                }
            `,
        },
        "Named",
    )
}

test "class_generic_bound_rejects_after_substituting_other_type_params" {
    AssertHasAnyMessage(
        {
            "class_generic_bound_rejects_after_substituting_other_type_params.baml": `
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
            `,
        },
        ["Slot<string>", "type mismatch"],
    )
}

test "class_generic_bound_rejects_function_type_argument_bound" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_rejects_function_type_argument_bound.baml": `
                class CallbackBox<T extends (x: int) -> int> {
                    cb: T
                }

                function main() -> int {
                    return 1
                }
            `,
        },
        "is not an interface",
    )
}

test "class_generic_bound_rejects_non_matching_function_type_arguments" {
    AssertHasAnyMessage(
        {
            "class_generic_bound_rejects_non_matching_function_type_arguments.baml": `
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
            `,
        },
        ["(x: int) -> int", "type mismatch"],
    )
}

test "class_generic_bound_is_enforced_in_parameter_annotations" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_is_enforced_in_parameter_annotations.baml": `
                interface Named {
                    name: string
                }

                class Box<T extends Named> {
                    value: T
                }

                function takes_bad(x: Box<int>) -> int {
                    return 1
                }
            `,
        },
        "Named",
    )
}

test "class_generic_bound_is_enforced_in_class_field_annotations" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_is_enforced_in_class_field_annotations.baml": `
                interface Named {
                    name: string
                }

                class Box<T extends Named> {
                    value: T
                }

                class Bad {
                    value: Box<int>
                }
            `,
        },
        "Named",
    )
}

test "class_generic_bound_is_enforced_in_type_aliases" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_is_enforced_in_type_aliases.baml": `
                interface Named {
                    name: string
                }

                class Box<T extends Named> {
                    value: T
                }

                type Bad = Box<int>
            `,
        },
        "Named",
    )
}

test "class_generic_bound_is_enforced_in_optional_type_aliases" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_is_enforced_in_optional_type_aliases.baml": `
                interface Named {
                    name: string
                }

                class Box<T extends Named> {
                    value: T
                }

                type Bad = Box<int>?
            `,
        },
        "Named",
    )
}

test "class_generic_bound_is_enforced_when_alias_is_used" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_is_enforced_when_alias_is_used.baml": `
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
            `,
        },
        "Named",
    )
}

test "class_generic_bound_is_enforced_in_optional_annotations" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_is_enforced_in_optional_annotations.baml": `
                interface Named {
                    name: string
                }

                class Box<T extends Named> {
                    value: T
                }

                function takes_bad(value: Box<int>?) -> int {
                    return 1
                }
            `,
        },
        "Named",
    )
}

test "class_generic_bound_is_enforced_in_function_throws_annotations" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_is_enforced_in_function_throws_annotations.baml": `
                interface Named {
                    name: string
                }

                class Box<T extends Named> {
                    value: T
                }

                function bad() -> int throws Box<int> {
                    return 1
                }
            `,
        },
        "Named",
    )
}

test "class_generic_bound_is_enforced_in_lambda_annotations" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_is_enforced_in_lambda_annotations.baml": `
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
            `,
        },
        "Named",
    )
}

test "class_generic_bound_is_enforced_in_lambda_throws_annotations" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_is_enforced_in_lambda_throws_annotations.baml": `
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
            `,
        },
        "Named",
    )
}

test "class_generic_bound_is_enforced_in_let_else_patterns" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_is_enforced_in_let_else_patterns.baml": `
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
            `,
        },
        "Named",
    )
}

test "class_generic_bound_is_enforced_in_match_patterns" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_is_enforced_in_match_patterns.baml": `
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
            `,
        },
        "Named",
    )
}

test "class_generic_bound_is_enforced_in_interface_field_annotations" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_is_enforced_in_interface_field_annotations.baml": `
                interface Named {
                    name: string
                }

                class Box<T extends Named> {
                    value: T
                }

                interface Bad {
                    value: Box<int>
                }
            `,
        },
        "Named",
    )
}

test "class_generic_bound_is_enforced_in_required_interface_method_annotations" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_is_enforced_in_required_interface_method_annotations.baml": `
                interface Named {
                    name: string
                }

                class Box<T extends Named> {
                    value: T
                }

                interface Bad {
                    function f(self, value: Box<int>) -> Box<int> throws never
                }
            `,
        },
        "Named",
    )
}

test "method_generic_bound_referencing_class_param_is_enforced" {
    root.compiler.AssertHasMessage(
        {
            "method_generic_bound_referencing_class_param_is_enforced.baml": `
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
            `,
        },
        "Eq<int>",
    )
}
