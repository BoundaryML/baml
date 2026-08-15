//! Executable oracles for the nine sealed reflection-kind views.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn kind_union_is_exhaustive_and_classifies_all_nine_kinds() {
    let output = baml_test!(
        r#"
        class Foo { value int }
        enum Color { Red Blue }
        interface Marker {}
        type Callback = (x: int, label: string) -> bool throws never

        function classify(t: type) -> string {
            match (t.kind()) {
                baml.reflect.class.Type => "class",
                baml.reflect.enum.Type => "enum",
                baml.reflect.union.Type => "union",
                baml.reflect.literal.Type => "literal",
                baml.reflect.array.Type => "array",
                baml.reflect.map.Type => "map",
                baml.reflect.interface.Type => "interface",
                baml.reflect.primitive.Type => "primitive",
                baml.reflect.function.Type => "function"
            }
        }

        function main() -> bool {
            classify(type.of<Foo>()) == "class"
                && classify(type.of<Color>()) == "enum"
                && classify(type.of<int | string>()) == "union"
                && classify(type.of<"fixed">()) == "literal"
                && classify(type.of<int[]>()) == "array"
                && classify(type.of<map<string, int>>()) == "map"
                && classify(type.of<Marker>()) == "interface"
                && classify(type.of<int>()) == "primitive"
                && classify(type.of<Callback>()) == "function"
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn kind_casts_are_nullable_identity_preserving_views() {
    let output = baml_test!(
        r#"
        class Foo { value int }

        function main() -> bool throws string {
            let t = type.of<Foo>();
            let view = t.as_class() ?? throw "expected class kind";
            let optional_count = t.as_class()?.fields()?.length();

            t.kind().as_type() == t
                && view.as_type() == t
                && view == (t.as_class() ?? throw "expected the same class kind")
                && optional_count == 1
                && t.as_enum() == null
                && type.of<int>().as_class() == null
                && type.of<int>().as_primitive()?.as_type() == type.of<int>()
                && type.of<Foo>() is type
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn class_and_enum_readback_preserves_schema_metadata() {
    let output = baml_test!(
        r#"
        /// Person docs
        class Person {
            /// Name docs
            name string @alias("full_name") @description("Name description") @custom("field-extra")
            @@alias("PersonAlias")
            @@description("Person description")
            @@custom("class-extra")
        }

        /// Color docs
        enum Color {
            /// Red docs
            Red @alias("rouge") @description("Red description") @custom("variant-extra")
            Blue
            @@alias("ColorAlias")
            @@description("Color description")
            @@custom("enum-extra")
        }

        function main() -> bool throws string {
            let class_view = type.of<Person>().as_class() ?? throw "class";
            let class_meta = class_view.meta();
            let field = class_view.fields()[0];
            let enum_view = type.of<Color>().as_enum() ?? throw "enum";
            let enum_meta = enum_view.meta();
            let red = enum_view.values()[0];

            class_meta.alias == "PersonAlias"
                && class_meta.description == "Person description"
                && class_meta.docstring == "Person docs"
                && class_meta.other.get("custom") == "class-extra"
                && field.name == "name"
                && field.type == type.of<string>()
                && field.meta.alias == "full_name"
                && field.meta.description == "Name description"
                && field.meta.docstring == "Name docs"
                && field.meta.other.get("custom") == "field-extra"
                && enum_meta.alias == "ColorAlias"
                && enum_meta.description == "Color description"
                && enum_meta.docstring == "Color docs"
                && enum_meta.other.get("custom") == "enum-extra"
                && red.name == "Red"
                && red.meta.alias == "rouge"
                && red.meta.description == "Red description"
                && red.meta.docstring == "Red docs"
                && red.meta.other.get("custom") == "variant-extra"
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn nested_type_walker_and_kind_specific_readback_work_end_to_end() {
    let output = baml_test!(
        r#"
        interface Marker {
            function mark(self) -> string throws never
        }

        class Foo {
            value int
            implements Marker {
                function mark(self) -> string throws never { "foo" }
            }
        }
        enum Color { Red }
        type Callback = (x: int, label: string) -> bool throws string

        function walk(t: type) -> int {
            match (t.kind()) {
                let class_view: baml.reflect.class.Type => 1,
                let enum_view: baml.reflect.enum.Type => 1,
                let union_view: baml.reflect.union.Type => {
                    let count = 1;
                    for let member in union_view.member_types() {
                        count += walk(member)
                    }
                    count
                },
                let literal_view: baml.reflect.literal.Type => 1,
                let array_view: baml.reflect.array.Type => 1 + walk(array_view.element_type()),
                let map_view: baml.reflect.map.Type => 1,
                let interface_view: baml.reflect.interface.Type => 1,
                let primitive_view: baml.reflect.primitive.Type => 1,
                let function_view: baml.reflect.function.Type => 1
            }
        }

        function read_views(
            union_view: baml.reflect.union.Type,
            array_view: baml.reflect.array.Type,
            map_view: baml.reflect.map.Type,
            interface_view: baml.reflect.interface.Type,
            function_view: baml.reflect.function.Type
        ) -> bool throws never {
            let params = function_view.params();
            let function_schema = match (type.of<baml.reflect.function.Type>().kind()) {
                let class_view: baml.reflect.class.Type => class_view,
                _ => return false
            };
            union_view.member_types().length() == 2
                && array_view.element_type() == type.of<Foo>()
                && map_view.key_type() == type.of<string>()
                && map_view.value_type() == type.of<Foo>()
                && interface_view.implemented_by(type.of<Foo>())
                && params.length() == 2
                && params[0].name == "x"
                && params[0].type == type.of<int>()
                && params[0].optional == false
                && params[1].name == "label"
                && params[1].type == type.of<string>()
                && function_view.return_type() == type.of<bool>()
                && function_schema.fields().length() == 0
        }

        function intrinsic_checks() -> bool throws never {
            type.of_value(1) == type.of<int>()
                && type.of_value(1).as_primitive() != null
                && type.of<Foo>().to_string() == "Foo"
                && type.of<int>().to_string() == "int"
                && type.of<Color>().to_string() != ""
                && type.of<int | string>().to_string() != ""
                && type.of<"fixed">().to_string() != ""
                && type.of<Foo[]>().to_string() != ""
                && type.of<map<string, Foo>>().to_string() != ""
                && type.of<Marker>().to_string() != ""
                && type.of<Callback>().to_string() != ""
        }

        function casts_are_never_throwing(t: type) -> bool throws never {
            t.as_class() != null
                && t.as_enum() == null
                && t.as_union() == null
                && t.as_literal() == null
                && t.as_array() == null
                && t.as_map() == null
                && t.as_interface() == null
                && t.as_primitive() == null
                && t.as_function() == null
        }

        function all_positive_kind_casts_work() -> bool throws never {
            type.of<Foo>().as_class() != null
                && type.of<Color>().as_enum() != null
                && type.of<int | string>().as_union() != null
                && type.of<"fixed">().as_literal() != null
                && type.of<Foo[]>().as_array() != null
                && type.of<map<string, Foo>>().as_map() != null
                && type.of<Marker>().as_interface() != null
                && type.of<int>().as_primitive() != null
                && type.of<Callback>().as_function() != null
        }

        function kind_identity(t: type) -> bool throws never {
            t.kind().as_type() == t
        }

        function every_kind_preserves_identity() -> bool throws never {
            kind_identity(type.of<Foo>())
                && kind_identity(type.of<Color>())
                && kind_identity(type.of<int | string>())
                && kind_identity(type.of<"fixed">())
                && kind_identity(type.of<Foo[]>())
                && kind_identity(type.of<map<string, Foo>>())
                && kind_identity(type.of<Marker>())
                && kind_identity(type.of<int>())
                && kind_identity(type.of<Callback>())
        }

        function main() -> bool throws unknown {
            let union_view = type.of<int | string>().as_union() ?? throw "union";
            let array_view = type.of<Foo[]>().as_array() ?? throw "array";
            let map_view = type.of<map<string, Foo>>().as_map() ?? throw "map";
            let interface_view = type.of<Marker>().as_interface() ?? throw "interface";
            let function_view = type.of<Callback>().as_function() ?? throw "function";

            walk(type.of<(Foo | string[])[]>()) == 5
                && read_views(union_view, array_view, map_view, interface_view, function_view)
                && intrinsic_checks()
                && casts_are_never_throwing(type.of<Foo>())
                && all_positive_kind_casts_work()
                && every_kind_preserves_identity()
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}
