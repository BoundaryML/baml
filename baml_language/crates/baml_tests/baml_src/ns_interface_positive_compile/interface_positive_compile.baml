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
            "two_interfaces_same_field_same_type_are_separate_namespaces.baml": `
                interface Named { name: string }
                interface Labeled { name: string }

                class Item {
                    name: string
                    implements Named {}
                    implements Labeled {}
                }
            `,
        },
    )
}

test "impl_may_narrow_interface_method_throws" {
    root.compiler.AssertCompiles(
        {
            "impl_may_narrow_interface_method_throws.baml": `
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
            `,
        },
    )
}

test "method_signature_match_is_ok" {
    AssertNoInterfaceErrors(
        {
            "method_signature_match_is_ok.baml": `
                interface Adder {
                    function add(self, a: int, b: int) -> int throws never
                }
                class GoodAdder {
                    implements Adder {
                        function add(self, a: int, b: int) -> int { return a + b }
                    }
                }
            `,
        },
    )
}

test "method_signature_match_preserves_throws_annotation" {
    AssertNoInterfaceErrors(
        {
            "method_signature_match_preserves_throws_annotation.baml": `
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
            `,
        },
    )
}

test "interface_field_union_order_is_exactly_equivalent" {
    root.compiler.AssertCompiles(
        {
            "interface_field_union_order_is_exactly_equivalent.baml": `
                interface Measured {
                    value: int | string
                }
                class Reading {
                    value: string | int
                    implements Measured {}
                }
            `,
        },
    )
}

test "generic_interface_field_union_order_is_exactly_equivalent" {
    root.compiler.AssertCompiles(
        {
            "generic_interface_field_union_order_is_exactly_equivalent.baml": `
                interface Measured<T, E> {
                    value: T | E
                }
                class Reading<T, E> {
                    value: E | T
                    implements Measured<T, E> {}
                }
            `,
        },
    )
}

test "generic_class_unannotated_self_is_parameterized" {
    root.compiler.AssertCompiles(
        {
            "generic_class_unannotated_self_is_parameterized.baml": `
                function consume<X>(v: X) -> int { return 1 }
                class Wrap<T> {
                    value: T
                    function use_self(self) -> int { return consume<Wrap<T>>(self) }
                }
            `,
        },
    )
}

test "mixed_interface_and_concrete_destructures_do_not_require_wildcard" {
    root.compiler.AssertCompiles(
        {
            "mixed_interface_and_concrete_destructures_do_not_require_wildcard.baml": `
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
            `,
        },
    )
}

test "concrete_then_interface_destructure_covers_remaining_implementor_values" {
    root.compiler.AssertCompiles(
        {
            "concrete_then_interface_destructure_covers_remaining_implementor_values.baml": `
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
            `,
        },
    )
}

test "mixed_interface_and_concrete_destructures_project_aliased_fields" {
    root.compiler.AssertCompiles(
        {
            "mixed_interface_and_concrete_destructures_project_aliased_fields.baml": `
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
            `,
        },
    )
}

test "mixed_interface_and_generic_concrete_destructures_project_class_type_args" {
    root.compiler.AssertCompiles(
        {
            "mixed_interface_and_generic_concrete_destructures_project_class_type_args.baml": `
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
            `,
        },
    )
}

test "mixed_interface_and_generic_concrete_destructures_project_swapped_class_type_args" {
    root.compiler.AssertCompiles(
        {
            "mixed_interface_and_generic_concrete_destructures_project_swapped_class_type_args.baml": `
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
            `,
        },
    )
}

test "union_of_generic_interfaces_allows_mixed_interface_and_concrete_destructures" {
    root.compiler.AssertCompiles(
        {
            "union_of_generic_interfaces_allows_mixed_interface_and_concrete_destructures.baml": `
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
            `,
        },
    )
}

test "union_of_interfaces_allows_mixed_interface_and_concrete_destructures" {
    root.compiler.AssertCompiles(
        {
            "union_of_interfaces_allows_mixed_interface_and_concrete_destructures.baml": `
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
            `,
        },
    )
}

test "llm_function_can_return_interface_type" {
    AssertNoInterfaceErrors(
        {
            "llm_function_can_return_interface_type.baml": `
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
                    prompt: \`
                        Identify the animal from the description: \${description}.
                        \${ctx.output_format()}
                    \`
                }
            `,
        },
    )
}

test "llm_function_returning_interface_enumerates_implementors_in_schema" {
    AssertNoInterfaceErrors(
        {
            "llm_function_returning_interface_enumerates_implementors_in_schema.baml": `
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
                    prompt: \`
                        Identify the animal: \${description}.
                        \${ctx.output_format()}
                    \`
                }
            `,
        },
    )
}

test "llm_function_with_interface_array_return_compiles" {
    AssertNoInterfaceErrors(
        {
            "llm_function_with_interface_array_return_compiles.baml": `
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
                    prompt: \`
                        Identify every animal mentioned in \${description}.
                        \${ctx.output_format()}
                    \`
                }
            `,
        },
    )
}

test "llm_function_with_interface_in_union_return_compiles" {
    AssertNoInterfaceErrors(
        {
            "llm_function_with_interface_in_union_return_compiles.baml": `
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
                    prompt: \`
                        If \${description} clearly identifies an animal, return one.
                        Otherwise, paraphrase the description.
                        \${ctx.output_format()}
                    \`
                }
            `,
        },
    )
}

test "llm_function_takes_interface_typed_parameter_compiles" {
    AssertNoInterfaceErrors(
        {
            "llm_function_takes_interface_typed_parameter_compiles.baml": `
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
                    prompt: \`
                        Describe the animal named \${a.name}.
                        \${ctx.output_format()}
                    \`
                }
            `,
        },
    )
}

test "interface_field_links_can_surround_methods" {
    root.compiler.AssertCompiles(
        {
            "interface_field_links_can_surround_methods.baml": `
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
            `,
        },
    )
}

test "interface_requires_same_named_fields_of_different_types_is_allowed" {
    root.compiler.AssertCompiles(
        {
            "interface_requires_same_named_fields_of_different_types_is_allowed.baml": `
                interface X {
                    id: string
                }
                interface Y {
                    id: int
                }
                interface Z requires X, Y {}
            `,
        },
    )
}

test "generic_required_parent_args_accept_alias_equivalent_type" {
    root.compiler.AssertCompiles(
        {
            "generic_required_parent_args_accept_alias_equivalent_type.baml": `
                type Texty = string | int

                interface Parent<T> {}
                interface Child requires Parent<int | string> {}

                class Box {
                    implements Parent<Texty> {}
                    implements Child {}
                }
            `,
        },
    )
}

test "out_of_body_method_callable_on_instance" {
    root.compiler.AssertCompiles(
        {
            "out_of_body_method_callable_on_instance.baml": `
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
            `,
        },
    )
}

test "same_interface_assignment_is_ok" {
    root.compiler.AssertCompiles(
        {
            "same_interface_assignment_is_ok.baml": `
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
            `,
        },
    )
}

test "form1_coexists_with_concrete_impl_for_different_class" {
    AssertNoInterfaceErrors(
        {
            "form1_coexists_with_concrete_impl_for_different_class.baml": `
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
            `,
        },
    )
}

test "non_overlapping_disjoint_generic_args_are_ok" {
    AssertNoInterfaceErrors(
        {
            "non_overlapping_disjoint_generic_args_are_ok.baml": `
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
            `,
        },
    )
}

test "cross_file_non_overlapping_impls_are_ok" {
    root.compiler.AssertCompiles(
        {
            "a.baml": `
                interface Printable {
                    function display(self) -> string throws never
                }
                class Apple {}
                implements Printable for Apple {
                    function display(self) -> string { return "apple" }
                }
            `,
            "b.baml": `
                class Banana {}
                implements Printable for Banana {
                    function display(self) -> string { return "banana" }
                }
            `,
        },
    )
}

test "non_overlapping_generic_receiver_rules_for_different_classes_are_ok" {
    AssertNoInterfaceErrors(
        {
            "non_overlapping_generic_receiver_rules_for_different_classes_are_ok.baml": `
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
            `,
        },
    )
}

test "bounded_type_var_rule_disjoint_from_concrete_not_satisfying_bound" {
    AssertNoInterfaceErrors(
        {
            "bounded_type_var_rule_disjoint_from_concrete_not_satisfying_bound.baml": `
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
            `,
        },
    )
}

test "homogeneous_blanket_disjoint_from_heterogeneous_ground" {
    AssertNoInterfaceErrors(
        {
            "homogeneous_blanket_disjoint_from_heterogeneous_ground.baml": `
                interface Printable { function display(self) -> string throws never }
                class Pair<A, B> { first: A second: B }
                implements<T> Printable for Pair<T, T> {
                    function display(self) -> string { return "tt" }
                }
                implements Printable for Pair<int, string> {
                    function display(self) -> string { return "is" }
                }
            `,
        },
    )
}

test "nested_generic_disjoint_ground_arg_is_ok" {
    AssertNoInterfaceErrors(
        {
            "nested_generic_disjoint_ground_arg_is_ok.baml": `
                interface Marker {}
                class Box<X> { v: X }
                class Pair<A, B> { first: A second: B }
                implements<T> Marker for Box<Pair<int, T>> {}
                implements<U> Marker for Box<Pair<string, U>> {}
            `,
        },
    )
}

test "nested_union_in_generic_arg_disjoint_is_ok" {
    AssertNoInterfaceErrors(
        {
            "nested_union_in_generic_arg_disjoint_is_ok.baml": `
                interface Marker {}
                class Box<X> { v: X }
                class Pair<A, B> { first: A second: B }
                implements<T> Marker for Box<Pair<int | T, bool>> {}
                implements Marker for Box<Pair<string | float, bool>> {}
            `,
        },
    )
}

test "box_int_vs_box_literal_is_disjoint" {
    root.compiler.AssertCompiles(
        {
            "box_int_vs_box_literal_is_disjoint.baml": `
                interface Marker {}
                class Box<X> { v: X }
                implements Marker for Box<int> {}
                implements Marker for Box<1> {}
            `,
        },
    )
}

test "literal_union_subset_of_base_is_disjoint" {
    root.compiler.AssertCompiles(
        {
            "literal_union_subset_of_base_is_disjoint.baml": `
                interface Marker {}
                class Box<X> { v: X }
                implements Marker for Box<1 | 2> {}
                implements Marker for Box<int> {}
            `,
        },
    )
}

test "union_member_vs_unrelated_single_is_disjoint" {
    root.compiler.AssertCompiles(
        {
            "union_member_vs_unrelated_single_is_disjoint.baml": `
                interface Marker {}
                class Box<X> { v: X }
                class C {}
                class D {}
                implements<T> Marker for Box<D | T> {}
                implements Marker for Box<C> {}
            `,
        },
    )
}

test "box_unknown_vs_box_int_is_disjoint" {
    root.compiler.AssertCompiles(
        {
            "box_unknown_vs_box_int_is_disjoint.baml": `
                interface Marker {}
                class Box<X> { v: X }
                implements Marker for Box<unknown> {}
                implements Marker for Box<int> {}
            `,
        },
    )
}

test "aliases_to_distinct_classes_are_disjoint" {
    root.compiler.AssertCompiles(
        {
            "aliases_to_distinct_classes_are_disjoint.baml": `
                interface Marker {}
                class C {}
                class D {}
                type AliasC = C
                type AliasD = D
                implement Marker for AliasC {}
                implement Marker for AliasD {}
            `,
        },
    )
}

test "namespaced_class_can_implement_root_qualified_interface" {
    root.compiler.AssertCompiles(
        {
            "main.baml": `
                function main() -> string {
                    let d = pets.Dog {}
                    return d.describe()
                }
            `,
            "ns_animals/animals.baml": `
                interface Animal {
                    function sound(self) -> string throws never
                    function describe(self) -> string throws never {
                        return "I say " + self.sound()
                    }
                }
            `,
            "ns_pets/pets.baml": `
                class Dog {
                    implements root.animals.Animal {
                        function sound(self) -> string { return "Woof" }
                    }
                }
            `,
        },
    )
}

test "namespaced_class_method_body_resolves_root_qualified_interface_type" {
    root.compiler.AssertCompiles(
        {
            "main.baml": `
                function main() -> string {
                    let d = pets.Dog {}
                    return d.greet()
                }
            `,
            "ns_a/a.baml": `
                interface Named {
                    function name(self) -> string throws never
                }
            `,
            "ns_b/b.baml": `
                interface Greeter {
                    function greet(self) -> string throws never
                }
            `,
            "ns_pets/pets.baml": `
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
            `,
        },
    )
}

test "qualified_generic_constructor_resolves_child_namespace_type_args" {
    root.compiler.AssertCompiles(
        {
            "main.baml": `
                function explicit() -> root.b.Box<int> {
                    return root.b.Box<int> { value: 42 }
                }

                function inferred() -> root.b.Box<int> {
                    return root.b.Box { value: 42 }
                }
            `,
            "ns_b/box.baml": `
                class Box<T> {
                    value: T
                }
            `,
        },
    )
}

test "resolvable_type_arg_in_implements_clause_is_ok" {
    AssertNoInterfaceErrors(
        {
            "resolvable_type_arg_in_implements_clause_is_ok.baml": `
                interface Container<T> {
                    function size(self) -> int throws never
                }
                class Box {
                    items: int[]
                    implements Container<int> {
                        function size(self) -> int { return 0 }
                    }
                }
            `,
        },
    )
}

test "ambiguous_method_namespace_qualified_fix_compiles" {
    root.compiler.AssertCompiles(
        {
            "main.baml": `
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
            `,
            "ns_zoo/zoo.baml": `interface Animal { function speak(self) -> string throws never }`,
            "ns_farm/farm.baml": `interface Animal { function speak(self) -> string throws never }`,
        },
    )
}

test "wf3_impl_type_param_in_interface_args_is_accepted" {
    root.compiler.AssertCompiles(
        {
            "wf3_impl_type_param_in_interface_args_is_accepted.baml": `
                interface Tagged<T> { function tag(self) -> string throws never }
                class Holder { v: int }
                implements<T> Tagged<T> for Holder {
                    function tag(self) -> string { return "h" }
                }
                function main() -> string {
                    let a: Tagged<int> = Holder { v: 1 }
                    return a.tag()
                }
            `,
        },
    )
}

test "wf3_pure_diamond_single_method_across_namespaces_no_false_ambiguity" {
    root.compiler.AssertCompiles(
        {
            "main.baml": `function main() -> string { let w = b.W {} return w.f() }`,
            "ns_a/a.baml": `interface Base { function f(self) -> string throws never }`,
            "ns_b/b.baml": `
                interface Derived requires root.a.Base {}
                class W {
                    implements root.b.Derived {}
                    implements root.a.Base { function f(self) -> string { return "from-base" } }
                }
            `,
        },
    )
}

test "wf3_three_level_requires_chain_single_method_no_false_ambiguity" {
    root.compiler.AssertCompiles(
        {
            "main.baml": `function main() -> string { let w = c.W {} return w.f() }`,
            "ns_a/a.baml": `interface A { function f(self) -> string throws never }`,
            "ns_b/b.baml": `interface B requires root.a.A {}`,
            "ns_c/c.baml": `
                interface C requires root.b.B {}
                class W {
                    implements root.c.C {}
                    implements root.b.B {}
                    implements root.a.A { function f(self) -> string { return "deep" } }
                }
            `,
        },
    )
}

test "wf3_throws_union_order_is_equivalent" {
    root.compiler.AssertCompiles(
        {
            "wf3_throws_union_order_is_equivalent.baml": `
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
            `,
        },
    )
}

test "wf3_throws_alias_is_resolved_in_signature_match" {
    root.compiler.AssertCompiles(
        {
            "wf3_throws_alias_is_resolved_in_signature_match.baml": `
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
            `,
        },
    )
}

test "wf3_throws_covariant_narrower_is_allowed" {
    root.compiler.AssertCompiles(
        {
            "wf3_throws_covariant_narrower_is_allowed.baml": `
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
            `,
        },
    )
}

test "arithmetic_out_of_body_user_impl_is_ok" {
    root.compiler.AssertCompiles(
        {
            "arithmetic_out_of_body_user_impl_is_ok.baml": `
                class Counter { n: int }
                implement baml.ops.Add<int> for Counter {
                    type Output = int
                    function add(self, rhs: int) -> int throws never { self.n + rhs }
                }
                function f(c: Counter) -> int { c + 1 }
            `,
        },
    )
}

test "arithmetic_on_interface_existential_is_ok" {
    root.compiler.AssertCompiles(
        {
            "arithmetic_on_interface_existential_is_ok.baml": `
                function f(x: baml.ops.Add<int, Output = int>, a: int) -> int {
                    x + a
                }
            `,
        },
    )
}

test "arithmetic_on_userclass_interface_existential_is_ok" {
    root.compiler.AssertCompiles(
        {
            "arithmetic_on_userclass_interface_existential_is_ok.baml": `
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
            `,
        },
    )
}

test "arithmetic_on_bounded_typevar_with_pinned_output_is_ok" {
    root.compiler.AssertCompiles(
        {
            "arithmetic_on_bounded_typevar_with_pinned_output_is_ok.baml": `
                function f<T extends baml.ops.Add<int, Output = int>>(x: T) -> int {
                    x + 1
                }
            `,
        },
    )
}
