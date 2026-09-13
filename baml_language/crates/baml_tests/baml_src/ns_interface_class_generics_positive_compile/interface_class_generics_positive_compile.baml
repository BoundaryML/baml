// Positive class-generic interface fixtures migrated from interfaces_class_generics.rs.
// Isolated reflected packages preserve the original fixture boundaries.

test "class_generic_bound_through_requires_chain_exposes_all_members" {
    root.compiler.AssertCompiles(
        {
            "class_generic_bound_through_requires_chain_exposes_all_members.baml": "\n        interface Named {\n            name: string\n        }\n\n        interface Person requires Named {\n            occupation: string\n        }\n\n        class Employee {\n            name: string\n            occupation: string\n            implements Named {}\n            implements Person {}\n        }\n\n        class Holder<T extends Person> {\n            value: T\n\n            function label(self) -> string {\n                return self.value.name + \":\" + self.value.occupation\n            }\n        }\n\n        function main() -> Holder<Employee> {\n            return Holder<Employee> {\n                value: Employee { name: \"Ada\", occupation: \"Engineer\" }\n            }\n        }\n        ",
        },
    )
}

test "class_generic_bound_is_visible_inside_lambda_body" {
    root.compiler.AssertCompiles(
        {
            "class_generic_bound_is_visible_inside_lambda_body.baml": "\n        interface Named {\n            name: string\n        }\n\n        class Dog {\n            name: string\n            implements Named {}\n        }\n\n        class Box<T extends Named> {\n            value: T\n\n            function get_name(self) -> string {\n                let read = () -> string {\n                    return self.value.name\n                }\n                return read()\n            }\n        }\n\n        function main() -> string {\n            return Box<Dog> { value: Dog { name: \"Rex\" } }.get_name()\n        }\n        ",
        },
    )
}

test "class_generic_bound_substitutes_other_class_type_params" {
    root.compiler.AssertCompiles(
        {
            "class_generic_bound_substitutes_other_class_type_params.baml": "\n        interface Slot<T> {\n            value: T\n        }\n\n        class IntSlot {\n            value: int\n            implements Slot<int> {}\n        }\n\n        class Holder<T extends Slot<U>, U> {\n            item: T\n\n            function read(self) -> U {\n                return self.item.value\n            }\n        }\n\n        function main() -> Holder<IntSlot, int> {\n            return Holder<IntSlot, int> { item: IntSlot { value: 1 } }\n        }\n        ",
        },
    )
}

test "class_generic_bound_allows_bounded_type_var_as_type_arg_in_class_field" {
    root.compiler.AssertCompiles(
        {
            "class_generic_bound_allows_bounded_type_var_as_type_arg_in_class_field.baml": "\n        interface Marker {}\n\n        class Box<T extends Marker> {\n            value: T\n        }\n\n        class Outer<T extends Marker> {\n            inner: Box<T>\n        }\n\n        function main() -> int {\n            return 1\n        }\n        ",
        },
    )
}
