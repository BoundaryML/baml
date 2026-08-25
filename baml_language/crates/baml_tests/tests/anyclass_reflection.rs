//! End-to-end coverage for the read-only `baml.AnyClass` reflection surface.

use baml_compiler_diagnostics::Severity;
use baml_tests::{
    baml_test,
    stdlib_prefix::{check_user_files, setup_test_db},
};
use bex_engine::BexExternalValue;

fn compile_error_codes(source: &str) -> Vec<String> {
    let db = setup_test_db(source);
    check_user_files(&db)
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

#[test]
fn requires_any_class_rejects_a_primitive_implementor() {
    let errors = compile_error_codes(
        r#"
        interface Tagged requires baml.AnyClass {
            function tag(self) -> string throws never
        }

        implements Tagged for int {
            function tag(self) -> string { "primitive" }
        }
        "#,
    );
    assert!(
        errors.iter().any(|code| code == "E0125"),
        "expected E0125, got {errors:?}"
    );
}

#[test]
fn any_class_blanket_bound_does_not_expose_members_on_nonclasses() {
    let errors = compile_error_codes(
        r#"
        interface Wrapped {
            function tag(self) -> string throws never
        }

        class Box<T> { value T }

        implements<T extends baml.AnyClass> Wrapped for Box<T> {
            function tag(self) -> string { "wrapped:" + self.value.type().to_string() }
        }

        function bad(value: Box<int>) -> string {
            value.tag()
        }
        "#,
    );
    assert!(
        errors.iter().any(|code| code == "E0007"),
        "expected E0007, got {errors:?}"
    );
}

#[tokio::test]
async fn requires_and_bounded_impl_membership_agree_for_real_classes() {
    let output = baml_test!(
        r#"
        class Record { label string }
        class Box<T> { value T }

        interface Tagged requires baml.AnyClass {
            function tag(self) -> string throws never
        }

        implements Tagged for Record {
            function tag(self) -> string { "record:" + self.label }
        }

        interface Wrapped {
            function wrapped(self) -> string throws never
        }

        implements<T extends baml.AnyClass> Wrapped for Box<T> {
            function wrapped(self) -> string { "wrapped:" + self.value.name() }
        }

        function main() -> bool {
            let record = Record { label: "ok" }
            let boxed = Box<Record> { value: record }
            record.tag() == "record:ok"
                && boxed.wrapped() == "wrapped:Record"
                && reflect.Type.of<Record>().implements(reflect.Type.of<Tagged>())
                && reflect.Type.of<Box<Record>>().implements(reflect.Type.of<Wrapped>())
                && !reflect.Type.of<Box<int>>().implements(reflect.Type.of<Wrapped>())
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn runtime_minted_class_narrows_and_exercises_the_complete_surface() {
    let output = baml_test!(
        r##"
        client TestClient = openai.ResponsesClient.new(
            model = "gpt-4o-mini",
            api_key = "test-key",
            base_url = "http://localhost:1234",
        );

        function Extract<T>(text: string) -> T {
            client: TestClient
            prompt: `Extract ${text}.\n${ctx.output_format}`
        }

        function mismatch_is_catchable(value: baml.AnyClass) -> bool throws never {
            let ignored = value.get<int>("first") catch (e) {
                baml.errors.TypeMismatch { message } => {
                    return message.includes("field `VetrecLike.first`")
                }
            }
            false
        }

        function main() -> bool {
            let optional_string = reflect.Type.of<string>().optional().as_type()
            let runtime_t = reflect.class.new("VetrecLike", {
                "first": optional_string.meta(description = "first field"),
                "second": optional_string,
                "third": optional_string,
                "fourth": optional_string,
                "fifth": optional_string,
            })
            let opaque = Extract$parse<unreflect(runtime_t.as_type())>(
                `{"first":"one","second":"two","third":null,"fourth":"four","fifth":"five"}`,
            )
            let record: baml.AnyClass = opaque else {
                throw "Expected class"
            }

            let first_field: reflect.class.Field = record.get_field("first") else {
                throw "Expected first field"
            }
            let fields = record.list_fields()
            record.name() == "VetrecLike"
                && record.type() == runtime_t.as_type()
                && record.attributes().alias == null
                && record.has_field("first")
                && record.has_field("third")
                && !record.has_field("missing")
                && record.get<string?>("first") == "one"
                && record.get<string?>("second") == "two"
                && record.get<string?>("third") == null
                && record.get<string?>("fourth") == "four"
                && record.get<string?>("fifth") == "five"
                && record.get<string?>("missing") == null
                && fields.length() == 5
                && first_field.name == "first"
                && first_field.type == optional_string
                && first_field.metadata().alias == null
                && first_field.value<string?>() == "one"
                && mismatch_is_catchable(record)
        }
        "##
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn membership_is_class_only_with_the_ratified_kind_view_exception() {
    let output = baml_test!(
        r#"
        class Point { x int }
        enum Color { Red }
        interface Marker {}
        type Callback = (value: int) -> string throws never

        function narrows(value: unknown) -> bool throws never {
            let class_value: baml.AnyClass = value else {
                return false
            }
            class_value.type() == reflect.Type.of_value(value)
        }

        function main() -> bool throws unknown {
            let numbers = { "one": 1 }
            let class_kind = reflect.Type.of<Point>().as_class() ?? throw "class kind"
            let enum_kind = reflect.Type.of<Color>().as_enum() ?? throw "enum kind"
            let union_kind = reflect.Type.of<int | string>().as_union() ?? throw "union kind"
            let literal_kind = reflect.Type.of<"fixed">().as_literal() ?? throw "literal kind"
            let array_kind = reflect.Type.of<int[]>().as_array() ?? throw "array kind"
            let map_kind = reflect.Type.of<map<string, int>>().as_map() ?? throw "map kind"
            let interface_kind = reflect.Type.of<Marker>().as_interface() ?? throw "interface kind"
            let primitive_kind = reflect.Type.of<int>().as_primitive() ?? throw "primitive kind"
            let function_kind = reflect.Type.of<Callback>().as_function() ?? throw "function kind"

            narrows(Point { x: 1 })
                && narrows(class_kind)
                && !narrows(enum_kind)
                && !narrows(union_kind)
                && !narrows(literal_kind)
                && !narrows(array_kind)
                && !narrows(map_kind)
                && !narrows(interface_kind)
                && !narrows(primitive_kind)
                && !narrows(function_kind)
                && !narrows(1)
                && !narrows("text")
                && !narrows(Color.Red)
                && !narrows([1, 2])
                && !narrows(numbers)
                && numbers.get("one") == 1
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn reflected_membership_and_static_field_handles_agree_with_narrowing() {
    let output = baml_test!(
        r#"
        /// Point docs
        class Point {
            x int @description("x coordinate")
            @@alias("point")
        }

        function main() -> bool throws unknown {
            let any_class_t = reflect.Type.of<baml.AnyClass>()
            let any_class_view = any_class_t.as_interface() ?? throw "AnyClass interface"
            let point = Point { x: 7 }
            let narrowed: baml.AnyClass = point else {
                throw "Expected class"
            }
            let field: reflect.class.Field = narrowed.get_field("x") else {
                throw "Expected x"
            }
            let point_type: reflect.class.Type = reflect.Type.of<Point>().as_class() else {
                throw "Point type"
            }
            let type_side_field = point_type.fields()[0]

            reflect.Type.of<Point>().implements(any_class_t)
                && any_class_view.implemented_by(reflect.Type.of<Point>())
                && reflect.Type.of<reflect.class.Type>().implements(any_class_t)
                && !reflect.Type.of<reflect.enum.Type>().implements(any_class_t)
                && !reflect.Type.of<int>().implements(any_class_t)
                && narrowed.name() == "Point"
                && narrowed.attributes().alias == "point"
                && field.type == reflect.Type.of<int>()
                && field.meta.description == "x coordinate"
                && field.metadata().description == "x coordinate"
                && field.value<int>() == 7
                && type_side_field.value<int>() == null
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn concrete_members_keep_precedence_until_explicitly_narrowed() {
    let output = baml_test!(
        r#"
        class ExistingFields {
            name string
            type int
            attributes string
        }

        class ExistingMethods {
            function get(self, name: string) -> string { "get:" + name }
            function get_field(self, name: string) -> string { "get_field:" + name }
            function has_field(self, name: string) -> string { "has_field:" + name }
            function list_fields(self) -> string { "list_fields" }
            function name(self) -> string { "name" }
            function type(self) -> string { "type" }
            function attributes(self) -> string { "attributes" }
        }

        interface ExistingOutOfBody<T> {
            function get(self) -> T throws unknown
        }

        class OutOfBodyBox<T> {
            value: T

            function new(value: T) -> OutOfBodyBox<T> throws never {
                OutOfBodyBox<T> { value: value }
            }
        }

        implements<T> ExistingOutOfBody<T> for OutOfBodyBox<T> {
            function get(self) -> T {
                self.value
            }
        }

        function main() -> bool throws unknown {
            let fields = ExistingFields {
                name: "field-name",
                type: 7,
                attributes: "field-attributes",
            }
            let methods = ExistingMethods {}
            let boxed = OutOfBodyBox.new("out-of-body")
            let boxed_value = boxed.get()
            let reflected: baml.AnyClass = fields else {
                throw "Expected class"
            }

            fields.name == "field-name"
                && fields.type == 7
                && fields.attributes == "field-attributes"
                && methods.get("x") == "get:x"
                && methods.get_field("x") == "get_field:x"
                && methods.has_field("x") == "has_field:x"
                && methods.list_fields() == "list_fields"
                && methods.name() == "name"
                && methods.type() == "type"
                && methods.attributes() == "attributes"
                && reflect.Type.of_value(boxed_value) == reflect.Type.of<string>()
                && reflected.name() == "ExistingFields"
                && reflected.get<string>("name") == "field-name"
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}
