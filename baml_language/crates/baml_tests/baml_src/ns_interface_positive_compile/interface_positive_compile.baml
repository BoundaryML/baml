// Positive interface compiler fixtures migrated from interfaces.rs.
// Each source stays isolated because the fixtures intentionally reuse declaration names.

function AssertNoInterfaceErrors(files: map<string, string>) -> void throws never {
    let interface_codes = [
        "E0112",
        "E0113",
        "E0114",
        "E0115",
        "E0116",
        "E0117",
        "E0118",
        "E0119",
        "E0120",
        "E0121",
        "E0122",
        "E0123",
        "E0124",
        "E0125",
        "E0126",
        "E0127",
        "E0128",
        "E0129",
        "E0130",
        "E0131",
        "E0132",
    ];
    let diagnostics = root.compiler.CompileDiagnostics(files);
    let interface_diagnostics = diagnostics.filter((diagnostic) -> bool {
        interface_codes.includes(diagnostic.code)
    });
    assert.equal(interface_diagnostics.length(), 0)
}

test "two_interfaces_same_field_same_type_are_separate_namespaces" {
    root.compiler.AssertCompiles(
        {
            "two_interfaces_same_field_same_type_are_separate_namespaces.baml": "\n        interface Named { name: string }\n        interface Labeled { name: string }\n\n        class Item {\n            name: string\n            implements Named {}\n            implements Labeled {}\n        }\n        ",
        },
    )
}

test "impl_may_narrow_interface_method_throws" {
    root.compiler.AssertCompiles(
        {
            "impl_may_narrow_interface_method_throws.baml": "\n        class IoError {\n            message: string\n        }\n\n        interface Fallible {\n            function run(self) -> string throws IoError\n        }\n\n        class Worker {\n            implements Fallible {\n                function run(self) -> string {\n                    return \"ok\"\n                }\n            }\n        }\n        ",
        },
    )
}

test "method_signature_match_is_ok" {
    AssertNoInterfaceErrors(
        {
            "method_signature_match_is_ok.baml": "\n        interface Adder {\n            function add(self, a: int, b: int) -> int throws never\n        }\n        class GoodAdder {\n            implements Adder {\n                function add(self, a: int, b: int) -> int { return a + b }\n            }\n        }\n        ",
        },
    )
}

test "method_signature_match_preserves_throws_annotation" {
    AssertNoInterfaceErrors(
        {
            "method_signature_match_preserves_throws_annotation.baml": "\n        class IoError {\n            message: string\n        }\n\n        interface Fallible {\n            function run(self) -> string throws IoError\n        }\n\n        class Worker {\n            implements Fallible {\n                function run(self) -> string throws IoError {\n                    return \"ok\"\n                }\n            }\n        }\n        ",
        },
    )
}

test "interface_field_union_order_is_exactly_equivalent" {
    root.compiler.AssertCompiles(
        {
            "interface_field_union_order_is_exactly_equivalent.baml": "\n        interface Measured {\n            value: int | string\n        }\n        class Reading {\n            value: string | int\n            implements Measured {}\n        }\n        ",
        },
    )
}

test "generic_interface_field_union_order_is_exactly_equivalent" {
    root.compiler.AssertCompiles(
        {
            "generic_interface_field_union_order_is_exactly_equivalent.baml": "\n        interface Measured<T, E> {\n            value: T | E\n        }\n        class Reading<T, E> {\n            value: E | T\n            implements Measured<T, E> {}\n        }\n        ",
        },
    )
}

test "generic_class_unannotated_self_is_parameterized" {
    root.compiler.AssertCompiles(
        {
            "generic_class_unannotated_self_is_parameterized.baml": "\n        function consume<X>(v: X) -> int { return 1 }\n        class Wrap<T> {\n            value: T\n            function use_self(self) -> int { return consume<Wrap<T>>(self) }\n        }\n        ",
        },
    )
}

test "mixed_interface_and_concrete_destructures_do_not_require_wildcard" {
    root.compiler.AssertCompiles(
        {
            "mixed_interface_and_concrete_destructures_do_not_require_wildcard.baml": "\n        interface Animal {\n            name: string\n        }\n        class Dog {\n            name: string\n            breed: string\n            implements Animal {}\n        }\n\n        function describe(a: Animal) -> string {\n            return match (a) {\n                Dog { breed: \"Lab\" } => \"lab\"\n                Animal { name: string } => \"animal\"\n            }\n        }\n        ",
        },
    )
}

test "concrete_then_interface_destructure_covers_remaining_implementor_values" {
    root.compiler.AssertCompiles(
        {
            "concrete_then_interface_destructure_covers_remaining_implementor_values.baml": "\n        interface Animal {\n            friendly: bool\n        }\n        class Dog {\n            friendly: bool\n            breed: string\n            implements Animal {}\n        }\n\n        function describe(a: Animal) -> string {\n            return match (a) {\n                Dog { friendly: true } => \"friendly dog\"\n                Animal { friendly: true } => \"other friendly animal\"\n                Animal { friendly: false } => \"other animal\"\n            }\n        }\n        ",
        },
    )
}

test "mixed_interface_and_concrete_destructures_project_aliased_fields" {
    root.compiler.AssertCompiles(
        {
            "mixed_interface_and_concrete_destructures_project_aliased_fields.baml": "\n        interface Toggle {\n            on: bool\n        }\n        class Widget {\n            label: string\n            enabled: bool\n            implements Toggle {\n                on as enabled\n            }\n        }\n\n        function describe(t: Toggle) -> string {\n            return match (t) {\n                Widget { enabled: true } => \"on\"\n                Toggle { on: true } => \"other on\"\n                Toggle { on: false } => \"off\"\n            }\n        }\n        ",
        },
    )
}

test "mixed_interface_and_generic_concrete_destructures_project_class_type_args" {
    root.compiler.AssertCompiles(
        {
            "mixed_interface_and_generic_concrete_destructures_project_class_type_args.baml": "\n        interface Slot<T> {\n            value: T\n        }\n        class Pair<L, R> {\n            left: L\n            right: R\n            implements Slot<L> {\n                value as left\n            }\n        }\n\n        function describe(s: Slot<int>) -> string {\n            return match (s) {\n                Pair<int, string> { right: \"seven\" } => \"pair\"\n                Slot<int> { value: int } => \"slot\"\n            }\n        }\n        ",
        },
    )
}

test "mixed_interface_and_generic_concrete_destructures_project_swapped_class_type_args" {
    root.compiler.AssertCompiles(
        {
            "mixed_interface_and_generic_concrete_destructures_project_swapped_class_type_args.baml": "\n        interface Cell<T> {\n            item: T\n        }\n        class Pair<L, R> {\n            left: L\n            right: R\n            implements Cell<R> {\n                item as right\n            }\n        }\n\n        function describe(c: Cell<string>) -> string {\n            return match (c) {\n                Pair<int, string> { left: 7 } => \"pair\"\n                Cell<string> { item: string } => \"cell\"\n            }\n        }\n        ",
        },
    )
}

test "union_of_generic_interfaces_allows_mixed_interface_and_concrete_destructures" {
    root.compiler.AssertCompiles(
        {
            "union_of_generic_interfaces_allows_mixed_interface_and_concrete_destructures.baml": "\n        interface Slot<T> {\n            value: T\n        }\n        class Pair<L, R> {\n            left: L\n            right: R\n            implements Slot<L> {\n                value as left\n            }\n        }\n\n        interface Cargo<T> {\n            payload: T\n        }\n        class Box<T> {\n            inner: T\n            label: string\n            implements Cargo<T> {\n                payload as inner\n            }\n        }\n\n        function describe(x: Slot<int> | Cargo<string>) -> string {\n            return match (x) {\n                Pair<int, string> { right: \"tag\" } => \"pair\"\n                Slot<int> { value: int } => \"slot\"\n                Box<string> { label: \"crate\" } => \"box\"\n                Cargo<string> { payload: string } => \"cargo\"\n            }\n        }\n        ",
        },
    )
}

test "union_of_interfaces_allows_mixed_interface_and_concrete_destructures" {
    root.compiler.AssertCompiles(
        {
            "union_of_interfaces_allows_mixed_interface_and_concrete_destructures.baml": "\n        interface Animal {\n            awake: bool\n        }\n        class Dog {\n            awake: bool\n            breed: string\n            implements Animal {}\n        }\n\n        interface Vehicle {\n            running: bool\n        }\n        class Car {\n            running: bool\n            make: string\n            implements Vehicle {}\n        }\n\n        function describe(x: Animal | Vehicle) -> string {\n            return match (x) {\n                Dog { awake: true } => \"dog\"\n                Animal { awake: true } => \"animal\"\n                Animal { awake: false } => \"animal\"\n                Car { running: true } => \"car\"\n                Vehicle { running: true } => \"vehicle\"\n                Vehicle { running: false } => \"vehicle\"\n            }\n        }\n        ",
        },
    )
}

test "llm_function_can_return_interface_type" {
    AssertNoInterfaceErrors(
        {
            "llm_function_can_return_interface_type.baml": "\n        interface Animal {\n            function speak(self) -> string throws never\n        }\n        class Dog {\n            implements Animal {\n                function speak(self) -> string { return \"Woof!\" }\n            }\n        }\n        class Cat {\n            implements Animal {\n                function speak(self) -> string { return \"Meow.\" }\n            }\n        }\n        function detect_animal(description: string) -> Animal {\n            client: GPT4o\n            prompt: `\n                Identify the animal from the description: ${description}.\n                ${ctx.output_format()}\n            `\n        }\n        ",
        },
    )
}

test "llm_function_returning_interface_enumerates_implementors_in_schema" {
    AssertNoInterfaceErrors(
        {
            "llm_function_returning_interface_enumerates_implementors_in_schema.baml": "\n        client<llm> GPT4o {\n            provider openai\n            options { model \"gpt-4o\" }\n        }\n        interface Animal {\n            function speak(self) -> string throws never\n        }\n        class Dog {\n            implements Animal {\n                function speak(self) -> string { return \"Woof!\" }\n            }\n        }\n        class Cat {\n            implements Animal {\n                function speak(self) -> string { return \"Meow.\" }\n            }\n        }\n        function detect_animal(description: string) -> Animal {\n            client: GPT4o\n            prompt: `\n                Identify the animal: ${description}.\n                ${ctx.output_format()}\n            `\n        }\n        ",
        },
    )
}

test "llm_function_with_interface_array_return_compiles" {
    AssertNoInterfaceErrors(
        {
            "llm_function_with_interface_array_return_compiles.baml": "\n        interface Animal {\n            function speak(self) -> string throws never\n        }\n        class Dog {\n            implements Animal {\n                function speak(self) -> string { return \"Woof!\" }\n            }\n        }\n        function detect_zoo(description: string) -> Animal[] {\n            client: GPT4o\n            prompt: `\n                Identify every animal mentioned in ${description}.\n                ${ctx.output_format()}\n            `\n        }\n        ",
        },
    )
}

test "llm_function_with_interface_in_union_return_compiles" {
    AssertNoInterfaceErrors(
        {
            "llm_function_with_interface_in_union_return_compiles.baml": "\n        interface Animal {\n            function speak(self) -> string throws never\n        }\n        class Dog {\n            implements Animal {\n                function speak(self) -> string { return \"Woof!\" }\n            }\n        }\n        function detect_or_describe(description: string) -> Animal | string {\n            client: GPT4o\n            prompt: `\n                If ${description} clearly identifies an animal, return one.\n                Otherwise, paraphrase the description.\n                ${ctx.output_format()}\n            `\n        }\n        ",
        },
    )
}

test "llm_function_takes_interface_typed_parameter_compiles" {
    AssertNoInterfaceErrors(
        {
            "llm_function_takes_interface_typed_parameter_compiles.baml": "\n        interface Animal {\n            name: string\n            function speak(self) -> string throws never\n        }\n        class Dog {\n            name: string\n            implements Animal {\n                function speak(self) -> string { return \"Woof!\" }\n            }\n        }\n        function describe_animal(a: Animal) -> string {\n            client: GPT4o\n            prompt: `\n                Describe the animal named ${a.name}.\n                ${ctx.output_format()}\n            `\n        }\n        ",
        },
    )
}

test "interface_field_links_can_surround_methods" {
    root.compiler.AssertCompiles(
        {
            "interface_field_links_can_surround_methods.baml": "\n        interface Widget {\n            id: string\n            function render(self) -> string throws never\n            label: string\n        }\n        class Button {\n            button_id: string\n            text: string\n            implements Widget {\n                id as button_id\n                function render(self) -> string { return \"<button>\" }\n                label as text\n            }\n        }\n        ",
        },
    )
}

test "interface_requires_same_named_fields_of_different_types_is_allowed" {
    root.compiler.AssertCompiles(
        {
            "interface_requires_same_named_fields_of_different_types_is_allowed.baml": "\n        interface X {\n            id: string\n        }\n        interface Y {\n            id: int\n        }\n        interface Z requires X, Y {}\n        ",
        },
    )
}

test "generic_required_parent_args_accept_alias_equivalent_type" {
    root.compiler.AssertCompiles(
        {
            "generic_required_parent_args_accept_alias_equivalent_type.baml": "\n        type Texty = string | int\n\n        interface Parent<T> {}\n        interface Child requires Parent<int | string> {}\n\n        class Box {\n            implements Parent<Texty> {}\n            implements Child {}\n        }\n        ",
        },
    )
}

test "out_of_body_method_callable_on_instance" {
    root.compiler.AssertCompiles(
        {
            "out_of_body_method_callable_on_instance.baml": "\n        interface Describable {\n            function describe(self) -> string throws never\n        }\n        class Car { make: string }\n        implements Describable for Car {\n            function describe(self) -> string { return self.make }\n        }\n        function run_describe() -> string {\n            let c = Car { make: \"Toyota\" }\n            return c.as<Describable>.describe()\n        }\n        ",
        },
    )
}

test "same_interface_assignment_is_ok" {
    root.compiler.AssertCompiles(
        {
            "same_interface_assignment_is_ok.baml": "\n        interface Animal {\n            function speak(self) -> string throws never\n        }\n        class Dog {\n            implements Animal {\n                function speak(self) -> string { return \"Woof!\" }\n            }\n        }\n        function ok() -> string {\n            let a: Animal = Dog {}\n            let b: Animal = a\n            return b.speak()\n        }\n        ",
        },
    )
}

test "form1_coexists_with_concrete_impl_for_different_class" {
    AssertNoInterfaceErrors(
        {
            "form1_coexists_with_concrete_impl_for_different_class.baml": "\n        interface Printable {\n            function display(self) -> string throws never\n        }\n        class Box<T> {\n            value: T\n        }\n        class Leaf {\n            label: string\n        }\n        implements<T> Printable for Box<T> {\n            function display(self) -> string { return \"box\" }\n        }\n        implements Printable for Leaf {\n            function display(self) -> string { return \"leaf\" }\n        }\n    ",
        },
    )
}

test "non_overlapping_disjoint_generic_args_are_ok" {
    AssertNoInterfaceErrors(
        {
            "non_overlapping_disjoint_generic_args_are_ok.baml": "\n        interface Printable {\n            function display(self) -> string throws never\n        }\n        class Pair<A, B> {\n            first: A\n            second: B\n        }\n        implements<T> Printable for Pair<int, T> {\n            function display(self) -> string { return \"int,t\" }\n        }\n        implements<U> Printable for Pair<string, U> {\n            function display(self) -> string { return \"string,u\" }\n        }\n        ",
        },
    )
}

test "cross_file_non_overlapping_impls_are_ok" {
    root.compiler.AssertCompiles(
        {
            "a.baml": "\n            interface Printable {\n                function display(self) -> string throws never\n            }\n            class Apple {}\n            implements Printable for Apple {\n                function display(self) -> string { return \"apple\" }\n            }\n            ",
            "b.baml": "\n            class Banana {}\n            implements Printable for Banana {\n                function display(self) -> string { return \"banana\" }\n            }\n            ",
        },
    )
}

test "non_overlapping_generic_receiver_rules_for_different_classes_are_ok" {
    AssertNoInterfaceErrors(
        {
            "non_overlapping_generic_receiver_rules_for_different_classes_are_ok.baml": "\n        interface Printable {\n            function display(self) -> string throws never\n        }\n        class Box<T> {\n            value: T\n        }\n        class Envelope<T> {\n            value: T\n        }\n        implements<T> Printable for Box<T> {\n            function display(self) -> string { return \"box\" }\n        }\n        implements<T> Printable for Envelope<T> {\n            function display(self) -> string { return \"envelope\" }\n        }\n        ",
        },
    )
}

test "bounded_type_var_rule_disjoint_from_concrete_not_satisfying_bound" {
    AssertNoInterfaceErrors(
        {
            "bounded_type_var_rule_disjoint_from_concrete_not_satisfying_bound.baml": "\n        interface Named {\n            name: string\n        }\n        interface Printable {\n            function display(self) -> string throws never\n        }\n        class User {\n            name: string\n        }\n        implements Printable for User {\n            function display(self) -> string { return \"user\" }\n        }\n        implements<T extends Named> Printable for T {\n            function display(self) -> string { return \"named\" }\n        }\n        ",
        },
    )
}

test "homogeneous_blanket_disjoint_from_heterogeneous_ground" {
    AssertNoInterfaceErrors(
        {
            "homogeneous_blanket_disjoint_from_heterogeneous_ground.baml": "\n        interface Printable { function display(self) -> string throws never }\n        class Pair<A, B> { first: A second: B }\n        implements<T> Printable for Pair<T, T> {\n            function display(self) -> string { return \"tt\" }\n        }\n        implements Printable for Pair<int, string> {\n            function display(self) -> string { return \"is\" }\n        }\n        ",
        },
    )
}

test "nested_generic_disjoint_ground_arg_is_ok" {
    AssertNoInterfaceErrors(
        {
            "nested_generic_disjoint_ground_arg_is_ok.baml": "\n        interface Marker {}\n        class Box<X> { v: X }\n        class Pair<A, B> { first: A second: B }\n        implements<T> Marker for Box<Pair<int, T>> {}\n        implements<U> Marker for Box<Pair<string, U>> {}\n        ",
        },
    )
}

test "nested_union_in_generic_arg_disjoint_is_ok" {
    AssertNoInterfaceErrors(
        {
            "nested_union_in_generic_arg_disjoint_is_ok.baml": "\n        interface Marker {}\n        class Box<X> { v: X }\n        class Pair<A, B> { first: A second: B }\n        implements<T> Marker for Box<Pair<int | T, bool>> {}\n        implements Marker for Box<Pair<string | float, bool>> {}\n        ",
        },
    )
}

test "box_int_vs_box_literal_is_disjoint" {
    root.compiler.AssertCompiles(
        {
            "box_int_vs_box_literal_is_disjoint.baml": "\n        interface Marker {}\n        class Box<X> { v: X }\n        implements Marker for Box<int> {}\n        implements Marker for Box<1> {}\n        ",
        },
    )
}

test "literal_union_subset_of_base_is_disjoint" {
    root.compiler.AssertCompiles(
        {
            "literal_union_subset_of_base_is_disjoint.baml": "\n        interface Marker {}\n        class Box<X> { v: X }\n        implements Marker for Box<1 | 2> {}\n        implements Marker for Box<int> {}\n        ",
        },
    )
}

test "union_member_vs_unrelated_single_is_disjoint" {
    root.compiler.AssertCompiles(
        {
            "union_member_vs_unrelated_single_is_disjoint.baml": "\n        interface Marker {}\n        class Box<X> { v: X }\n        class C {}\n        class D {}\n        implements<T> Marker for Box<D | T> {}\n        implements Marker for Box<C> {}\n        ",
        },
    )
}

test "box_unknown_vs_box_int_is_disjoint" {
    root.compiler.AssertCompiles(
        {
            "box_unknown_vs_box_int_is_disjoint.baml": "\n        interface Marker {}\n        class Box<X> { v: X }\n        implements Marker for Box<unknown> {}\n        implements Marker for Box<int> {}\n        ",
        },
    )
}

test "aliases_to_distinct_classes_are_disjoint" {
    root.compiler.AssertCompiles(
        {
            "aliases_to_distinct_classes_are_disjoint.baml": "\n        interface Marker {}\n        class C {}\n        class D {}\n        type AliasC = C\n        type AliasD = D\n        implement Marker for AliasC {}\n        implement Marker for AliasD {}\n        ",
        },
    )
}

test "namespaced_class_can_implement_root_qualified_interface" {
    root.compiler.AssertCompiles(
        {
            "main.baml": "\n                function main() -> string {\n                    let d = pets.Dog {}\n                    return d.describe()\n                }\n                ",
            "ns_animals/animals.baml": "\n                interface Animal {\n                    function sound(self) -> string throws never\n                    function describe(self) -> string throws never {\n                        return \"I say \" + self.sound()\n                    }\n                }\n                ",
            "ns_pets/pets.baml": "\n                class Dog {\n                    implements root.animals.Animal {\n                        function sound(self) -> string { return \"Woof\" }\n                    }\n                }\n                ",
        },
    )
}

test "namespaced_class_method_body_resolves_root_qualified_interface_type" {
    root.compiler.AssertCompiles(
        {
            "main.baml": "\n                function main() -> string {\n                    let d = pets.Dog {}\n                    return d.greet()\n                }\n                ",
            "ns_a/a.baml": "\n                interface Named {\n                    function name(self) -> string throws never\n                }\n                ",
            "ns_b/b.baml": "\n                interface Greeter {\n                    function greet(self) -> string throws never\n                }\n                ",
            "ns_pets/pets.baml": "\n                class Dog {\n                    implements root.a.Named {\n                        function name(self) -> string { return \"Rex\" }\n                    }\n                    implements root.b.Greeter {\n                        function greet(self) -> string {\n                            return \"Hi, I am \" + self.as<root.a.Named>.name()\n                        }\n                    }\n                }\n                ",
        },
    )
}

test "qualified_generic_constructor_resolves_child_namespace_type_args" {
    root.compiler.AssertCompiles(
        {
            "main.baml": "\n                function explicit() -> root.b.Box<int> {\n                    return root.b.Box<int> { value: 42 }\n                }\n\n                function inferred() -> root.b.Box<int> {\n                    return root.b.Box { value: 42 }\n                }\n                ",
            "ns_b/box.baml": "\n                class Box<T> {\n                    value: T\n                }\n                ",
        },
    )
}

test "resolvable_type_arg_in_implements_clause_is_ok" {
    AssertNoInterfaceErrors(
        {
            "resolvable_type_arg_in_implements_clause_is_ok.baml": "\n        interface Container<T> {\n            function size(self) -> int throws never\n        }\n        class Box {\n            items: int[]\n            implements Container<int> {\n                function size(self) -> int { return 0 }\n            }\n        }\n        ",
        },
    )
}

test "ambiguous_method_namespace_qualified_fix_compiles" {
    root.compiler.AssertCompiles(
        {
            "main.baml": "\n            class Parrot {\n                implements zoo.Animal {\n                    function speak(self) -> string { return \"squawk\" }\n                }\n                implements farm.Animal {\n                    function speak(self) -> string { return \"cluck\" }\n                }\n            }\n            function main() -> string {\n                let p = Parrot {}\n                return p.as<zoo.Animal>.speak()\n            }\n            ",
            "ns_zoo/zoo.baml": "interface Animal { function speak(self) -> string throws never }",
            "ns_farm/farm.baml": "interface Animal { function speak(self) -> string throws never }",
        },
    )
}

test "wf3_impl_type_param_in_interface_args_is_accepted" {
    root.compiler.AssertCompiles(
        {
            "wf3_impl_type_param_in_interface_args_is_accepted.baml": "\n        interface Tagged<T> { function tag(self) -> string throws never }\n        class Holder { v: int }\n        implements<T> Tagged<T> for Holder {\n            function tag(self) -> string { return \"h\" }\n        }\n        function main() -> string {\n            let a: Tagged<int> = Holder { v: 1 }\n            return a.tag()\n        }\n        ",
        },
    )
}

test "wf3_pure_diamond_single_method_across_namespaces_no_false_ambiguity" {
    root.compiler.AssertCompiles(
        {
            "main.baml": "function main() -> string { let w = b.W {} return w.f() }",
            "ns_a/a.baml": "interface Base { function f(self) -> string throws never }",
            "ns_b/b.baml": "\n            interface Derived requires root.a.Base {}\n            class W {\n                implements root.b.Derived {}\n                implements root.a.Base { function f(self) -> string { return \"from-base\" } }\n            }\n            ",
        },
    )
}

test "wf3_three_level_requires_chain_single_method_no_false_ambiguity" {
    root.compiler.AssertCompiles(
        {
            "main.baml": "function main() -> string { let w = c.W {} return w.f() }",
            "ns_a/a.baml": "interface A { function f(self) -> string throws never }",
            "ns_b/b.baml": "interface B requires root.a.A {}",
            "ns_c/c.baml": "\n            interface C requires root.b.B {}\n            class W {\n                implements root.c.C {}\n                implements root.b.B {}\n                implements root.a.A { function f(self) -> string { return \"deep\" } }\n            }\n            ",
        },
    )
}

test "wf3_throws_union_order_is_equivalent" {
    root.compiler.AssertCompiles(
        {
            "wf3_throws_union_order_is_equivalent.baml": "\n        class A { a: string }\n        class B { b: string }\n        interface Fallible {\n            function run(self) -> string throws A | B\n        }\n        class Worker {}\n        implements Fallible for Worker {\n            function run(self) -> string throws B | A {\n                throw A { a: \"x\" }\n            }\n        }\n        function main() -> string { return \"ok\" }\n        ",
        },
    )
}

test "wf3_throws_alias_is_resolved_in_signature_match" {
    root.compiler.AssertCompiles(
        {
            "wf3_throws_alias_is_resolved_in_signature_match.baml": "\n        class IoError { message: string }\n        type Err = IoError\n        interface Fallible {\n            function run(self) -> string throws Err\n        }\n        class Worker {}\n        implements Fallible for Worker {\n            function run(self) -> string throws IoError {\n                throw IoError { message: \"x\" }\n            }\n        }\n        function main() -> string { return \"ok\" }\n        ",
        },
    )
}

test "wf3_throws_covariant_narrower_is_allowed" {
    root.compiler.AssertCompiles(
        {
            "wf3_throws_covariant_narrower_is_allowed.baml": "\n        interface IError {\n            function describe(self) -> string throws never\n        }\n        class NetworkError {\n            msg: string\n            implements IError {\n                function describe(self) -> string { return \"net\" }\n            }\n        }\n        interface Fallible {\n            function run(self) -> string throws IError\n        }\n        class Worker {}\n        implements Fallible for Worker {\n            function run(self) -> string throws NetworkError {\n                throw NetworkError { msg: \"x\" }\n            }\n        }\n        function main() -> string { return \"ok\" }\n        ",
        },
    )
}

test "arithmetic_out_of_body_user_impl_is_ok" {
    root.compiler.AssertCompiles(
        {
            "arithmetic_out_of_body_user_impl_is_ok.baml": "\n        class Counter { n: int }\n        implement baml.ops.Add<int> for Counter {\n            type Output = int\n            function add(self, rhs: int) -> int throws never { self.n + rhs }\n        }\n        function f(c: Counter) -> int { c + 1 }\n        ",
        },
    )
}

test "arithmetic_on_interface_existential_is_ok" {
    root.compiler.AssertCompiles(
        {
            "arithmetic_on_interface_existential_is_ok.baml": "\n        function f(x: baml.ops.Add<int, Output = int>, a: int) -> int {\n            x + a\n        }\n        ",
        },
    )
}

test "arithmetic_on_userclass_interface_existential_is_ok" {
    root.compiler.AssertCompiles(
        {
            "arithmetic_on_userclass_interface_existential_is_ok.baml": "\n        class B { v: int }\n        class A {\n            v: int\n            implements baml.ops.Add<A> {\n                type Output = B\n                function add(self, rhs: A) -> B throws never { B { v: self.v + rhs.v } }\n            }\n        }\n        function f(x: baml.ops.Add<A, Output = B>, a: A) -> B {\n            x + a\n        }\n        ",
        },
    )
}

test "arithmetic_on_bounded_typevar_with_pinned_output_is_ok" {
    root.compiler.AssertCompiles(
        {
            "arithmetic_on_bounded_typevar_with_pinned_output_is_ok.baml": "\n        function f<T extends baml.ops.Add<int, Output = int>>(x: T) -> int {\n            x + 1\n        }\n        ",
        },
    )
}
