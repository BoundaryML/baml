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

        function classify(t: reflect.Type) -> string {
            match (t.kind()) {
                reflect.class.Type => "class",
                reflect.enum.Type => "enum",
                reflect.union.Type => "union",
                reflect.literal.Type => "literal",
                reflect.array.Type => "array",
                reflect.map.Type => "map",
                reflect.interface.Type => "interface",
                reflect.primitive.Type => "primitive",
                reflect.function.Type => "function"
            }
        }

        function main() -> bool {
            classify(reflect.Type.of<Foo>()) == "class"
                && classify(reflect.Type.of<Color>()) == "enum"
                && classify(reflect.Type.of<int | string>()) == "union"
                && classify(reflect.Type.of<"fixed">()) == "literal"
                && classify(reflect.Type.of<int[]>()) == "array"
                && classify(reflect.Type.of<map<string, int>>()) == "map"
                && classify(reflect.Type.of<Marker>()) == "interface"
                && classify(reflect.Type.of<int>()) == "primitive"
                && classify(reflect.Type.of<Callback>()) == "function"
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
            let t = reflect.Type.of<Foo>();
            let view = t.as_class() ?? throw "expected class kind";
            let optional_count = t.as_class()?.fields()?.length();

            t.kind().as_type() == t
                && view.as_type() == t
                && view == (t.as_class() ?? throw "expected the same class kind")
                && optional_count == 1
                && t.as_enum() == null
                && reflect.Type.of<int>().as_class() == null
                && reflect.Type.of<int>().as_primitive()?.as_type() == reflect.Type.of<int>()
                && reflect.Type.of<Foo>() is reflect.Type
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
            let class_view = reflect.Type.of<Person>().as_class() ?? throw "class";
            let class_meta = class_view.meta();
            let field = class_view.fields()[0];
            let enum_view = reflect.Type.of<Color>().as_enum() ?? throw "enum";
            let enum_meta = enum_view.meta();
            let red = enum_view.values()[0];

            class_meta.alias == "PersonAlias"
                && class_meta.description == "Person description"
                && class_meta.docstring == "Person docs"
                && class_meta.other.get("custom") == "class-extra"
                && field.name == "name"
                && field.type == reflect.Type.of<string>()
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

        function walk(t: reflect.Type) -> int {
            match (t.kind()) {
                let class_view: reflect.class.Type => 1,
                let enum_view: reflect.enum.Type => 1,
                let union_view: reflect.union.Type => {
                    let count = 1;
                    for let member in union_view.member_types() {
                        count += walk(member)
                    }
                    count
                },
                let literal_view: reflect.literal.Type => 1,
                let array_view: reflect.array.Type => 1 + walk(array_view.element_type()),
                let map_view: reflect.map.Type => 1,
                let interface_view: reflect.interface.Type => 1,
                let primitive_view: reflect.primitive.Type => 1,
                let function_view: reflect.function.Type => 1
            }
        }

        function read_views(
            union_view: reflect.union.Type,
            array_view: reflect.array.Type,
            map_view: reflect.map.Type,
            interface_view: reflect.interface.Type,
            function_view: reflect.function.Type
        ) -> bool throws reflect.errors.CompilationError {
            let params = function_view.params();
            let function_schema = match (reflect.Type.of<reflect.function.Type>().kind()) {
                let class_view: reflect.class.Type => class_view,
                _ => return false
            };
            union_view.member_types().length() == 2
                && array_view.element_type() == reflect.Type.of<Foo>()
                && map_view.key_type() == reflect.Type.of<string>()
                && map_view.value_type() == reflect.Type.of<Foo>()
                && interface_view.implemented_by(reflect.Type.of<Foo>())
                && params.length() == 2
                && params[0].name == "x"
                && params[0].type == reflect.Type.of<int>()
                && params[0].optional == false
                && params[1].name == "label"
                && params[1].type == reflect.Type.of<string>()
                && function_view.return_type() == reflect.Type.of<bool>()
                && function_schema.fields().length() == 0
        }

        function intrinsic_checks() -> bool throws never {
            reflect.Type.of_value(1) == reflect.Type.of<int>()
                && reflect.Type.of_value(1).as_primitive() != null
                && reflect.Type.of<Foo>().to_string() == "Foo"
                && reflect.Type.of<int>().to_string() == "int"
                && reflect.Type.of<Color>().to_string() != ""
                && reflect.Type.of<int | string>().to_string() != ""
                && reflect.Type.of<"fixed">().to_string() != ""
                && reflect.Type.of<Foo[]>().to_string() != ""
                && reflect.Type.of<map<string, Foo>>().to_string() != ""
                && reflect.Type.of<Marker>().to_string() != ""
                && reflect.Type.of<Callback>().to_string() != ""
        }

        function casts_are_never_throwing(t: reflect.Type) -> bool throws never {
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
            reflect.Type.of<Foo>().as_class() != null
                && reflect.Type.of<Color>().as_enum() != null
                && reflect.Type.of<int | string>().as_union() != null
                && reflect.Type.of<"fixed">().as_literal() != null
                && reflect.Type.of<Foo[]>().as_array() != null
                && reflect.Type.of<map<string, Foo>>().as_map() != null
                && reflect.Type.of<Marker>().as_interface() != null
                && reflect.Type.of<int>().as_primitive() != null
                && reflect.Type.of<Callback>().as_function() != null
        }

        function kind_identity(t: reflect.Type) -> bool throws never {
            t.kind().as_type() == t
        }

        function every_kind_preserves_identity() -> bool throws never {
            kind_identity(reflect.Type.of<Foo>())
                && kind_identity(reflect.Type.of<Color>())
                && kind_identity(reflect.Type.of<int | string>())
                && kind_identity(reflect.Type.of<"fixed">())
                && kind_identity(reflect.Type.of<Foo[]>())
                && kind_identity(reflect.Type.of<map<string, Foo>>())
                && kind_identity(reflect.Type.of<Marker>())
                && kind_identity(reflect.Type.of<int>())
                && kind_identity(reflect.Type.of<Callback>())
        }

        function main() -> bool throws unknown {
            let union_view = reflect.Type.of<int | string>().as_union() ?? throw "union";
            let array_view = reflect.Type.of<Foo[]>().as_array() ?? throw "array";
            let map_view = reflect.Type.of<map<string, Foo>>().as_map() ?? throw "map";
            let interface_view = reflect.Type.of<Marker>().as_interface() ?? throw "interface";
            let function_view = reflect.Type.of<Callback>().as_function() ?? throw "function";

            walk(reflect.Type.of<(Foo | string[])[]>()) == 5
                && read_views(union_view, array_view, map_view, interface_view, function_view)
                && intrinsic_checks()
                && casts_are_never_throwing(reflect.Type.of<Foo>())
                && all_positive_kind_casts_work()
                && every_kind_preserves_identity()
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

/// B-1582 item 2: decomposing a runtime type's view must keep the definition
/// overlay the enclosing value carries. Reading the nested enum's rows used to
/// hit `unreachable!("reflected enum … must be loaded")` in the VM.
#[tokio::test]
async fn nested_views_of_a_runtime_type_keep_its_definitions() {
    let output = baml_test!(
        r#"
        function main() -> string throws unknown {
            let choice = reflect.enum.new("Choice", ["FIRST", "SECOND"]).as_type();

            // Through a class field's array element.
            let root = reflect.class.new("Root", {
                "choices": choice.array().as_type(),
            }).as_type();
            let root_class = root.as_class() ?? throw "expected class";
            let field = root_class.fields().at(0) ?? throw "expected field";
            let field_type = field.type ?? throw "expected field type";
            let array = field_type.as_array() ?? throw "expected array";
            let from_array = array.element_type().as_enum() ?? throw "expected array enum";

            // Through a map value.
            let map_view = reflect.map.new(reflect.Type.of<string>(), choice).as_type().as_map()
                ?? throw "expected map";
            let from_map_value = map_view.value_type().as_enum() ?? throw "expected map enum";

            // Through a union member.
            let union_view = reflect.union.new([choice, reflect.Type.of<int>()]).as_type().as_union()
                ?? throw "expected union";
            let member = union_view.member_types().at(0) ?? throw "expected member";
            let from_union = member.as_enum() ?? throw "expected union enum";

            [
                (from_array.values().at(0) ?? throw "array rows").name,
                (from_map_value.values().at(1) ?? throw "map rows").name,
                (from_union.values().at(0) ?? throw "union rows").name,
            ].join("|")
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("FIRST|SECOND|FIRST".into()))
    );
}

/// The map *key* decomposes through the same helper as the value, so pin it too
/// — a runtime enum used as a map key stays introspectable.
#[tokio::test]
async fn map_key_type_view_keeps_runtime_definitions() {
    let output = baml_test!(
        r#"
        function main() -> string throws unknown {
            let choice = reflect.enum.new("Choice", ["FIRST", "SECOND"]).as_type();
            let map_view = reflect.map.new(choice, reflect.Type.of<int>()).as_type().as_map()
                ?? throw "expected map";
            let key_enum = map_view.key_type().as_enum() ?? throw "expected key enum";
            (key_enum.values().at(1) ?? throw "key rows").name
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::String("SECOND".into())));
}

/// A class nested inside a runtime-*package* type is reached the same way, and
/// its definition lives in the owning package rather than in a per-value
/// overlay. Reading it back must not fall through to the static type table.
#[tokio::test]
async fn nested_class_of_a_runtime_package_type_reads_back() {
    let output = baml_test!(
        r#"
        function main() -> string throws unknown {
            let pkg = reflect.Package.compile({
                "schema.baml": "class Leaf { name string } class Root { leaf Leaf }",
            })
            let root = pkg.get_class("root.Root") ?? throw "missing Root"
            let field = root.fields().at(0) ?? throw "missing field"
            let field_type = field.type ?? throw "missing field type"
            let leaf = field_type.as_class() ?? throw "expected a class view"
            let leaf_field = leaf.fields().at(0) ?? throw "expected a leaf field"
            leaf_field.name
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::String("name".into())));
}

/// A function view's own types are *produced* by reflection rather than
/// decomposed out of a value the caller already holds, so carrying the overlay
/// forward on the consumer side does not reach them: `package.functions()` and
/// `reflect.signature` built their `type` values with no definitions attached,
/// and reading a runtime package's enum back out of a parameter or a return
/// type hit `unreachable!("reflected enum … must be loaded")`. Both producers
/// now attach the owning package's declarations.
#[tokio::test]
async fn function_views_of_a_runtime_package_keep_its_definitions() {
    let output = baml_test!(
        r#"
        function main() -> string throws unknown {
            let pkg = reflect.Package.compile({
                "schema.baml": "enum Choice { FIRST SECOND } function pick(c: Choice) -> Choice { c }",
            })

            let view = pkg.functions().get("root.pick") ?? throw "missing pick"
            let returned = view.return_type().as_enum() ?? throw "expected a return enum"
            let returned_row = returned.values().at(0) ?? throw "no return rows"
            let param = view.params().at(0) ?? throw "expected a param"
            let param_enum = param.type.as_enum() ?? throw "expected a param enum"
            let param_row = param_enum.values().at(1) ?? throw "no param rows"

            let callable = pkg.get_function<reflect.AnyFunction<Returns = unknown, Throws = unknown>>(
                "root.pick",
            ) ?? throw "missing callable"
            let sig = reflect.signature(callable)
            let sig_returns = sig.returns.as_enum() ?? throw "expected a signature return enum"
            let sig_return_row = sig_returns.values().at(0) ?? throw "no signature return rows"
            let arg = sig.args.at(0) ?? throw "expected a signature arg"
            let sig_arg = arg.type.as_enum() ?? throw "expected a signature arg enum"
            let sig_arg_row = sig_arg.values().at(1) ?? throw "no signature arg rows"

            let parts = [
                returned_row.name,
                param_row.name,
                sig_return_row.name,
                sig_arg_row.name,
            ]
            parts.join("|")
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("FIRST|SECOND|FIRST|SECOND".into()))
    );
}
