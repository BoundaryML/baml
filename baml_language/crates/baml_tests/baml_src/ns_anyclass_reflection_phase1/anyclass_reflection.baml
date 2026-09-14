// Native ports of Phase-1 Rust integration tests.

test "requires_and_bounded_impl_membership_agree_for_real_classes" {
    let result = root.mixed_phase1_support.call(
        `
            class Record { label string }
            class Box<T> { value T }

            interface Tagged requires reflect.AnyClass {
                function tag(self) -> string throws never
            }

            implements Tagged for Record {
                function tag(self) -> string { "record:" + self.label }
            }

            interface Wrapped {
                function wrapped(self) -> string throws never
            }

            implements<T extends reflect.AnyClass> Wrapped for Box<T> {
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
        `,
        "root.main",
    );
    assert.equal(root.mixed_phase1_support.bool_result(result), true)
}

test "membership_is_class_only_and_kind_views_are_ordinary_classes" {
    let result = root.mixed_phase1_support.call(
        `
            class Point { x int }
            enum Color { Red }
            interface Marker {}
            type Callback = (value: int) -> string throws never

            function narrows(value: unknown) -> bool throws never {
                let class_value: reflect.AnyClass = value else {
                    return false
                }
                class_value.type() == reflect.Type.of_value(value)
            }

            function main() -> bool {
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

                // The nine kind views are ordinary wrapper classes, so every one
                // of them narrows to \`AnyClass\` like any other class instance.
                narrows(Point { x: 1 })
                    && narrows(class_kind)
                    && narrows(enum_kind)
                    && narrows(union_kind)
                    && narrows(literal_kind)
                    && narrows(array_kind)
                    && narrows(map_kind)
                    && narrows(interface_kind)
                    && narrows(primitive_kind)
                    && narrows(function_kind)
                    && !narrows(1)
                    && !narrows("text")
                    && !narrows(Color.Red)
                    && !narrows([1, 2])
                    && !narrows(numbers)
                    && numbers.get("one") == 1
            }
        `,
        "root.main",
    );
    assert.equal(root.mixed_phase1_support.bool_result(result), true)
}

test "reflected_membership_and_static_field_handles_agree_with_narrowing" {
    let result = root.mixed_phase1_support.call(
        `
            /// Point docs
            class Point {
                x int @description("x coordinate")
                @@alias("point")
            }

            function main() -> bool {
                let any_class_t = reflect.Type.of<reflect.AnyClass>()
                let any_class_view = any_class_t.as_interface() ?? throw "AnyClass interface"
                let point = Point { x: 7 }
                let narrowed: reflect.AnyClass = point else {
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
                    && reflect.Type.of<reflect.enum.Type>().implements(any_class_t)
                    && !reflect.Type.of<int>().implements(any_class_t)
                    && narrowed.name() == "Point"
                    && narrowed.attributes().alias == "point"
                    && field.type == reflect.Type.of<int>()
                    && field.meta.description == "x coordinate"
                    && field.metadata().description == "x coordinate"
                    && field.value<int>() == 7
                    && type_side_field.value<int>() == null
            }
        `,
        "root.main",
    );
    assert.equal(root.mixed_phase1_support.bool_result(result), true)
}

test "concrete_members_keep_precedence_until_explicitly_narrowed" {
    let result = root.mixed_phase1_support.call(
        `
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

            function main() -> bool {
                let fields = ExistingFields {
                    name: "field-name",
                    type: 7,
                    attributes: "field-attributes",
                }
                let methods = ExistingMethods {}
                let boxed = OutOfBodyBox.new("out-of-body")
                let boxed_value = boxed.get()
                let reflected: reflect.AnyClass = fields else {
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
        `,
        "root.main",
    );
    assert.equal(root.mixed_phase1_support.bool_result(result), true)
}
