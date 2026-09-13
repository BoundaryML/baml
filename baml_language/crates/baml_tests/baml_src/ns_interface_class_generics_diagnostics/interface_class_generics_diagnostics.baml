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
            "class_generic_bound_violation_is_compile_error_for_explicit_arg.baml": "\n        interface Named {\n            name: string\n        }\n\n        class Box<T extends Named> {\n            value: T\n        }\n\n        function main() -> Box<int> {\n            return Box<int> { value: 1 }\n        }\n        ",
        },
        "Named",
    )
}

test "class_generic_bound_violation_is_compile_error_for_inferred_arg" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_violation_is_compile_error_for_inferred_arg.baml": "\n        interface Named {\n            name: string\n        }\n\n        class Box<T extends Named> {\n            value: T\n        }\n\n        function main() -> int {\n            let bad = Box { value: 1 }\n            return 1\n        }\n        ",
        },
        "Named",
    )
}

test "class_generic_bound_rejects_union_bound" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_rejects_union_bound.baml": "\n        class Box<T extends int | string> {\n            value: T\n        }\n\n        function main() -> int {\n            return 1\n        }\n        ",
        },
        "is not an interface",
    )
}

test "class_generic_bound_rejects_type_outside_union" {
    AssertHasAnyMessage(
        {
            "class_generic_bound_rejects_type_outside_union.baml": "\n        class Box<T extends int | string> {\n            value: T\n        }\n\n        function main() -> Box<bool> {\n            return Box<bool> { value: true }\n        }\n        ",
        },
        ["int | string", "type mismatch"],
    )
}

test "class_generic_bound_rejects_container_and_optional_bounds" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_rejects_container_and_optional_bounds.baml": "\n        class ListBox<T extends int[]> {\n            value: T\n        }\n\n        function main() -> int {\n            return 1\n        }\n        ",
        },
        "is not an interface",
    )
}

test "class_generic_bound_rejects_wrong_compound_type" {
    AssertHasAnyMessage(
        {
            "class_generic_bound_rejects_wrong_compound_type.baml": "\n        class ListBox<T extends int[]> {\n            value: T\n        }\n\n        function main() -> ListBox<string[]> {\n            return ListBox<string[]> { value: [\"nope\"] }\n        }\n        ",
        },
        ["int[]", "type mismatch"],
    )
}

test "class_generic_bound_rejects_function_type_bound" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_rejects_function_type_bound.baml": "\n        interface Named {\n            name: string\n        }\n\n        class FunctionBox<T extends () -> Named> {\n            cb: T\n        }\n\n        function main() -> int {\n            return 1\n        }\n        ",
        },
        "is not an interface",
    )
}

test "class_generic_bound_rejects_interface_implementors_inside_invariant_containers" {
    AssertHasAnyMessage(
        {
            "class_generic_bound_rejects_interface_implementors_inside_invariant_containers.baml": "\n        interface Named {\n            name: string\n        }\n\n        class Dog {\n            name: string\n            implements Named {}\n        }\n\n        class ListBox<T extends Named[]> {\n            value: T\n        }\n\n        class MapBox<T extends map<string, Named>> {\n            value: T\n        }\n\n        function main() -> int {\n            let xs = ListBox<Dog[]> { value: [Dog { name: \"Rex\" }] }\n            let directory = MapBox<map<string, Dog>> { value: { \"rex\": Dog { name: \"Rex\" } } }\n            return 1\n        }\n        ",
        },
        ["Named[]", "map<string, Named>", "type mismatch"],
    )
}

test "class_generic_bound_rejects_nested_interface_bound_violation" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_rejects_nested_interface_bound_violation.baml": "\n        interface Named {\n            name: string\n        }\n\n        class ListBox<T extends Named[]> {\n            value: T\n        }\n\n        function main() -> ListBox<int[]> {\n            return ListBox<int[]> { value: [1] }\n        }\n        ",
        },
        "Named",
    )
}

test "class_generic_bound_rejects_after_substituting_other_type_params" {
    AssertHasAnyMessage(
        {
            "class_generic_bound_rejects_after_substituting_other_type_params.baml": "\n        interface Slot<T> {\n            value: T\n        }\n\n        class IntSlot {\n            value: int\n            implements Slot<int> {}\n        }\n\n        class Holder<T extends Slot<U>, U> {\n            item: T\n        }\n\n        function main() -> Holder<IntSlot, string> {\n            return Holder<IntSlot, string> { item: IntSlot { value: 1 } }\n        }\n        ",
        },
        ["Slot<string>", "type mismatch"],
    )
}

test "class_generic_bound_rejects_function_type_argument_bound" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_rejects_function_type_argument_bound.baml": "\n        class CallbackBox<T extends (x: int) -> int> {\n            cb: T\n        }\n\n        function main() -> int {\n            return 1\n        }\n        ",
        },
        "is not an interface",
    )
}

test "class_generic_bound_rejects_non_matching_function_type_arguments" {
    AssertHasAnyMessage(
        {
            "class_generic_bound_rejects_non_matching_function_type_arguments.baml": "\n        type StringCallback = (x: string) -> string\n\n        class CallbackBox<T extends (x: int) -> int> {\n            cb: T\n        }\n\n        function echo(x: string) -> string {\n            return x\n        }\n\n        function main() -> CallbackBox<StringCallback> {\n            return CallbackBox<StringCallback> { cb: echo }\n        }\n        ",
        },
        ["(x: int) -> int", "type mismatch"],
    )
}

test "class_generic_bound_is_enforced_in_parameter_annotations" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_is_enforced_in_parameter_annotations.baml": "\n        interface Named {\n            name: string\n        }\n\n        class Box<T extends Named> {\n            value: T\n        }\n\n        function takes_bad(x: Box<int>) -> int {\n            return 1\n        }\n        ",
        },
        "Named",
    )
}

test "class_generic_bound_is_enforced_in_class_field_annotations" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_is_enforced_in_class_field_annotations.baml": "\n        interface Named {\n            name: string\n        }\n\n        class Box<T extends Named> {\n            value: T\n        }\n\n        class Bad {\n            value: Box<int>\n        }\n        ",
        },
        "Named",
    )
}

test "class_generic_bound_is_enforced_in_type_aliases" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_is_enforced_in_type_aliases.baml": "\n        interface Named {\n            name: string\n        }\n\n        class Box<T extends Named> {\n            value: T\n        }\n\n        type Bad = Box<int>\n        ",
        },
        "Named",
    )
}

test "class_generic_bound_is_enforced_in_optional_type_aliases" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_is_enforced_in_optional_type_aliases.baml": "\n        interface Named {\n            name: string\n        }\n\n        class Box<T extends Named> {\n            value: T\n        }\n\n        type Bad = Box<int>?\n        ",
        },
        "Named",
    )
}

test "class_generic_bound_is_enforced_when_alias_is_used" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_is_enforced_when_alias_is_used.baml": "\n        interface Named {\n            name: string\n        }\n\n        class Box<T extends Named> {\n            value: T\n        }\n\n        type Bad = Box<int>?\n\n        function takes_bad(value: Bad) -> int {\n            return 1\n        }\n        ",
        },
        "Named",
    )
}

test "class_generic_bound_is_enforced_in_optional_annotations" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_is_enforced_in_optional_annotations.baml": "\n        interface Named {\n            name: string\n        }\n\n        class Box<T extends Named> {\n            value: T\n        }\n\n        function takes_bad(value: Box<int>?) -> int {\n            return 1\n        }\n        ",
        },
        "Named",
    )
}

test "class_generic_bound_is_enforced_in_function_throws_annotations" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_is_enforced_in_function_throws_annotations.baml": "\n        interface Named {\n            name: string\n        }\n\n        class Box<T extends Named> {\n            value: T\n        }\n\n        function bad() -> int throws Box<int> {\n            return 1\n        }\n        ",
        },
        "Named",
    )
}

test "class_generic_bound_is_enforced_in_lambda_annotations" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_is_enforced_in_lambda_annotations.baml": "\n        interface Named {\n            name: string\n        }\n\n        class Box<T extends Named> {\n            value: T\n        }\n\n        function main() -> int {\n            let f = () -> Box<int> {\n                return Box<int> { value: 1 }\n            }\n            return 1\n        }\n        ",
        },
        "Named",
    )
}

test "class_generic_bound_is_enforced_in_lambda_throws_annotations" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_is_enforced_in_lambda_throws_annotations.baml": "\n        interface Named {\n            name: string\n        }\n\n        class Box<T extends Named> {\n            value: T\n        }\n\n        function main() -> int {\n            let f = () -> int throws Box<int> {\n                return 1\n            }\n            return 1\n        }\n        ",
        },
        "Named",
    )
}

test "class_generic_bound_is_enforced_in_let_else_patterns" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_is_enforced_in_let_else_patterns.baml": "\n        interface Named {\n            name: string\n        }\n\n        class Box<T extends Named> {\n            value: T\n        }\n\n        function main(x: Box<Named>?) -> int {\n            let b: Box<int> = x else {\n                return 0\n            }\n            return 1\n        }\n        ",
        },
        "Named",
    )
}

test "class_generic_bound_is_enforced_in_match_patterns" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_is_enforced_in_match_patterns.baml": "\n        interface Named {\n            name: string\n        }\n\n        class Box<T extends Named> {\n            value: T\n        }\n\n        function main(x: Box<Named> | int) -> int {\n            return match (x) {\n                let b: Box<int> => 1,\n                _ => 0\n            }\n        }\n        ",
        },
        "Named",
    )
}

test "class_generic_bound_is_enforced_in_interface_field_annotations" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_is_enforced_in_interface_field_annotations.baml": "\n        interface Named {\n            name: string\n        }\n\n        class Box<T extends Named> {\n            value: T\n        }\n\n        interface Bad {\n            value: Box<int>\n        }\n        ",
        },
        "Named",
    )
}

test "class_generic_bound_is_enforced_in_required_interface_method_annotations" {
    root.compiler.AssertHasMessage(
        {
            "class_generic_bound_is_enforced_in_required_interface_method_annotations.baml": "\n        interface Named {\n            name: string\n        }\n\n        class Box<T extends Named> {\n            value: T\n        }\n\n        interface Bad {\n            function f(self, value: Box<int>) -> Box<int> throws never\n        }\n        ",
        },
        "Named",
    )
}

test "method_generic_bound_referencing_class_param_is_enforced" {
    root.compiler.AssertHasMessage(
        {
            "method_generic_bound_referencing_class_param_is_enforced.baml": "\n        interface Eq<T> { function equal(self, other: T) -> bool throws never }\n        class Wrapper<C> {\n            inner: C\n            function compare_with<U extends Eq<C>>(self, u: U) -> bool { return true }\n        }\n        class StrEq {\n            implements Eq<string> { function equal(self, other: string) -> bool { return true } }\n        }\n        function main() -> bool {\n            let w = Wrapper<int> { inner: 5 }\n            return w.compare_with<StrEq>(StrEq {})\n        }\n        ",
        },
        "Eq<int>",
    )
}
