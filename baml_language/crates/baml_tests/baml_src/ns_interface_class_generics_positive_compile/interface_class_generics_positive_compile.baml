// Positive class-generic interface fixtures migrated from interfaces_class_generics.rs.
// Isolated reflected packages preserve the original fixture boundaries.

test "class_generic_bound_through_requires_chain_exposes_all_members" {
    root.compiler.AssertCompiles(
        {
            "class_generic_bound_through_requires_chain_exposes_all_members.baml": `
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
            `,
        },
    )
}

test "class_generic_bound_is_visible_inside_lambda_body" {
    root.compiler.AssertCompiles(
        {
            "class_generic_bound_is_visible_inside_lambda_body.baml": `
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
            `,
        },
    )
}

test "class_generic_bound_substitutes_other_class_type_params" {
    root.compiler.AssertCompiles(
        {
            "class_generic_bound_substitutes_other_class_type_params.baml": `
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
            `,
        },
    )
}

test "class_generic_bound_allows_bounded_type_var_as_type_arg_in_class_field" {
    root.compiler.AssertCompiles(
        {
            "class_generic_bound_allows_bounded_type_var_as_type_arg_in_class_field.baml": `
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
            `,
        },
    )
}
