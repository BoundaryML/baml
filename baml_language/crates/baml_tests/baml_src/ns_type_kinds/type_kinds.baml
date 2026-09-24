// Executable oracles for the nine sealed reflection-kind views.

class Foo {
    value: int,

    implements Marker {
        function mark(self) -> string throws never {
            "foo"
        }
    }
}

enum KindColor {
    Red,
    Blue,
}

interface Marker {
    function mark(self) -> string throws never;
}

/// Person docs
class Person {
    /// Name docs
    name: string @alias("full_name") @description("Name description"),

    @@alias("PersonAlias")
    @@description("Person description")
}

/// Color docs
enum Color {
    /// Red docs
    Red @alias("rouge") @description("Red description"),
    Blue,

    @@alias("ColorAlias")
    @@description("Color description")
}

type Callback = (x: int, label: string) -> bool throws string;

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
        reflect.function.Type => "function",
    }
}

function walk(t: reflect.Type) -> int {
    match (t.kind()) {
        let class_view: reflect.class.Type => 1,
        let enum_view: reflect.enum.Type => 1,
        let union_view: reflect.union.Type => {
            let count = 1;
            for (let member in union_view.member_types()) {
                count += walk(member);
            }
            count
        },
        let literal_view: reflect.literal.Type => 1,
        let array_view: reflect.array.Type => 1 + walk(array_view.element_type()),
        let map_view: reflect.map.Type => 1,
        let interface_view: reflect.interface.Type => 1,
        let primitive_view: reflect.primitive.Type => 1,
        let function_view: reflect.function.Type => 1,
    }
}

function read_views(
    union_view: reflect.union.Type,
    array_view: reflect.array.Type,
    map_view: reflect.map.Type,
    interface_view: reflect.interface.Type,
    function_view: reflect.function.Type,
) -> bool throws reflect.errors.CompilationError {
    let params = function_view.params();
    let function_schema = match (reflect.Type.of<reflect.function.Type>().kind()) {
        let class_view: reflect.class.Type => class_view,
        _ => {
            return false;
        },
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
        && function_schema.fields().length() == 1
        && function_schema.fields()[0].name == "_ty"
}

function intrinsic_checks() -> bool throws never {
    reflect.Type.of_value(1) == reflect.Type.of<int>()
        && reflect.Type.of_value(1).as_primitive() != null
        && reflect.Type.of<Foo>().to_string() == "type_kinds.Foo"
        && reflect.Type.of<int>().to_string() == "int"
        && reflect.Type.of<KindColor>().to_string() != ""
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
        && reflect.Type.of<KindColor>().as_enum() != null
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
        && kind_identity(reflect.Type.of<KindColor>())
        && kind_identity(reflect.Type.of<int | string>())
        && kind_identity(reflect.Type.of<"fixed">())
        && kind_identity(reflect.Type.of<Foo[]>())
        && kind_identity(reflect.Type.of<map<string, Foo>>())
        && kind_identity(reflect.Type.of<Marker>())
        && kind_identity(reflect.Type.of<int>())
        && kind_identity(reflect.Type.of<Callback>())
}

test "kind_union_is_exhaustive_and_classifies_all_nine_kinds" {
    assert.equal(classify(reflect.Type.of<Foo>()), "class");
    assert.equal(classify(reflect.Type.of<KindColor>()), "enum");
    assert.equal(classify(reflect.Type.of<int | string>()), "union");
    assert.equal(classify(reflect.Type.of<"fixed">()), "literal");
    assert.equal(classify(reflect.Type.of<int[]>()), "array");
    assert.equal(classify(reflect.Type.of<map<string, int>>()), "map");
    assert.equal(classify(reflect.Type.of<Marker>()), "interface");
    assert.equal(classify(reflect.Type.of<int>()), "primitive");
    assert.equal(classify(reflect.Type.of<Callback>()), "function")
}

test "kind_casts_are_nullable_identity_preserving_views" {
    let t = reflect.Type.of<Foo>();
    let view = t.as_class() ?? throw "expected class kind";
    let optional_count = t.as_class()?.fields()?.length();

    assert.equal(t.kind().as_type(), t);
    assert.equal(view.as_type(), t);
    assert.equal(view, t.as_class() ?? throw "expected the same class kind");
    assert.equal(optional_count, 1);
    assert.is_true(t.as_enum() == null);
    assert.is_true(reflect.Type.of<int>().as_class() == null);
    assert.equal(reflect.Type.of<int>().as_primitive()?.as_type(), reflect.Type.of<int>());
    assert.is_true(reflect.Type.of<Foo>() is reflect.Type)
}

type EscapedLiteral = "a\"b\\c\n";

function literal_value(t: reflect.literal.Type) -> unknown throws never {
    t.value()
}

test "literal_values_preserve_primitive_values_and_types" {
    let text = reflect.Type.of<EscapedLiteral>().as_literal() ?? throw "literal";
    let integer = reflect.Type.of<-42>().as_literal() ?? throw "literal";
    let large = reflect.Type.of<90071992547409931234567890n>().as_literal() ?? throw "literal";
    let negative = reflect.Type.of<-90071992547409931234567890n>().as_literal() ?? throw "literal";
    let yes = reflect.Type.of<true>().as_literal() ?? throw "literal";
    let no = reflect.Type.of<false>().as_literal() ?? throw "literal";

    assert.equal(literal_value(text), "a\"b\\c\n");
    assert.equal(literal_value(integer), -42);
    assert.equal(literal_value(large), 90071992547409931234567890n);
    assert.equal(literal_value(negative), -90071992547409931234567890n);
    assert.equal(literal_value(yes), true);
    assert.equal(literal_value(no), false);
    assert.equal(reflect.Type.of_value(large.value()), reflect.Type.of<bigint>());
    assert.equal(reflect.Type.of_value(integer.value()), reflect.Type.of<int>());
    assert.is_true(reflect.Type.of<null>().as_literal() == null)
}

test "literal_values_preserve_enum_identity_not_alias_or_name_strings" {
    let red = reflect.Type.of<Color.Red>().as_literal() ?? throw "literal";
    let hidden = reflect.Type.of<WithSkippedVariant.Hidden>().as_literal() ?? throw "literal";
    assert.equal(red.value(), Color.Red);
    assert.equal(reflect.Type.of_value(red.value()), reflect.Type.of<Color>());
    assert.equal(reflect.enum.get_value(red.value()), "Red");
    assert.equal(hidden.value(), WithSkippedVariant.Hidden)
}

test "runtime_literal_constructors_roundtrip_values" {
    assert.equal(reflect.literal.new("hello").value(), "hello");
    assert.equal(reflect.literal.new(0).value(), 0);
    assert.equal(reflect.literal.new(-42).value(), -42);
    assert.equal(reflect.literal.new(true).value(), true);
    assert.equal(reflect.literal.new(false).value(), false)
}

test "runtime_package_enum_literal_values_keep_their_declaration" {
    let package = reflect.Package.compile(
        {
            "schema.baml": `enum Choice { FIRST SECOND } function pick() -> Choice.FIRST { Choice.FIRST }`,
        },
    );
    let fn_view = package.functions().get("root.pick") ?? throw "missing function";
    let literal = fn_view.return_type().as_literal() ?? throw "literal";
    let en = package.get_enum("root.Choice") ?? throw "missing enum";
    let value = literal.value();
    assert.equal(reflect.Type.of_value(value), en.as_type());
    assert.equal(reflect.enum.get_value(value), "FIRST")
}

test "class_and_enum_readback_preserves_schema_metadata" {
    let class_view = reflect.Type.of<Person>().as_class() ?? throw "class";
    let class_meta = class_view.meta();
    let field = class_view.fields()[0];
    let enum_view = reflect.Type.of<Color>().as_enum() ?? throw "enum";
    let enum_meta = enum_view.meta();
    let red = enum_view.values()[0];

    assert.equal(class_meta.alias, "PersonAlias");
    assert.equal(class_meta.description, "Person description");
    assert.equal(class_meta.docstring, "Person docs");
    assert.equal(field.name, "name");
    assert.equal(field.type, reflect.Type.of<string>());
    assert.equal(field.meta.alias, "full_name");
    assert.equal(field.meta.description, "Name description");
    assert.equal(field.meta.docstring, "Name docs");
    assert.equal(enum_meta.alias, "ColorAlias");
    assert.equal(enum_meta.description, "Color description");
    assert.equal(enum_meta.docstring, "Color docs");
    assert.equal(red.name, "Red");
    assert.equal(red.meta.alias, "rouge");
    assert.equal(red.meta.description, "Red description");
    assert.equal(red.meta.docstring, "Red docs")
}

class WithSkippedField {
    kept: string,
    hidden: string? @skip,
}

enum WithSkippedVariant {
    Kept,
    Hidden @skip,
}

test "skip_flag_reads_back_on_fields_and_variants_only" {
    let class_view = reflect.Type.of<WithSkippedField>().as_class() ?? throw "class";
    let enum_view = reflect.Type.of<WithSkippedVariant>().as_enum() ?? throw "enum";

    assert.equal(class_view.fields()[0].meta.skip, false);
    assert.is_true(class_view.fields()[1].meta.skip);
    assert.equal(enum_view.values()[0].meta.skip, false);
    assert.is_true(enum_view.values()[1].meta.skip);
    // A class or enum itself is never skipped.
    assert.equal(class_view.meta().skip, false);
    assert.equal(enum_view.meta().skip, false);
    // Rows built without the flag default to not skipped.
    assert.equal(reflect.enum.value("Fresh").meta.skip, false)
}

test "enum_rebuilt_from_reflected_rows_keeps_skip" {
    let enum_view = reflect.Type.of<WithSkippedVariant>().as_enum() ?? throw "enum";
    let rows: (string | reflect.enum.Value)[] = [];
    for (let row in enum_view.values()) {
        rows.push(row);
    }
    let rebuilt = reflect.enum.new("Rebuilt", rows).as_type().as_enum() ?? throw "rebuilt";
    assert.equal(rebuilt.values()[0].meta.skip, false);
    assert.is_true(rebuilt.values()[1].meta.skip)
}

test "nested_type_walker_and_kind_specific_readback_work_end_to_end" {
    let union_view = reflect.Type.of<int | string>().as_union() ?? throw "union";
    let array_view = reflect.Type.of<Foo[]>().as_array() ?? throw "array";
    let map_view = reflect.Type.of<map<string, Foo>>().as_map() ?? throw "map";
    let interface_view = reflect.Type.of<Marker>().as_interface() ?? throw "interface";
    let function_view = reflect.Type.of<Callback>().as_function() ?? throw "function";

    assert.equal(walk(reflect.Type.of<(Foo | string[])[]>()), 5);
    assert.is_true(read_views(union_view, array_view, map_view, interface_view, function_view));
    assert.is_true(intrinsic_checks());
    assert.is_true(casts_are_never_throwing(reflect.Type.of<Foo>()));
    assert.is_true(all_positive_kind_casts_work());
    assert.is_true(every_kind_preserves_identity())
}

test "nested_views_of_a_runtime_type_keep_its_definitions" {
    let choice = reflect.enum.new("Choice", ["FIRST", "SECOND"]).as_type();

    let root = reflect.class.new("Root", { "choices": choice.array().as_type() }).as_type();
    let root_class = root.as_class() ?? throw "expected class";
    let field = root_class.fields().at(0) ?? throw "expected field";
    let field_type = field.type ?? throw "expected field type";
    let array = field_type.as_array() ?? throw "expected array";
    let from_array = array.element_type().as_enum() ?? throw "expected array enum";

    let map_view = reflect.map.new(reflect.Type.of<string>(), choice).as_type().as_map() ?? throw "expected map";
    let from_map_value = map_view.value_type().as_enum() ?? throw "expected map enum";

    let union_view = reflect.union.new([choice, reflect.Type.of<int>()]).as_type().as_union()
        ?? throw "expected union";
    let member = union_view.member_types().at(0) ?? throw "expected member";
    let from_union = member.as_enum() ?? throw "expected union enum";

    assert.equal((from_array.values().at(0) ?? throw "array rows").name, "FIRST");
    assert.equal((from_map_value.values().at(1) ?? throw "map rows").name, "SECOND");
    assert.equal((from_union.values().at(0) ?? throw "union rows").name, "FIRST")
}

test "map_key_type_view_keeps_runtime_definitions" {
    let choice = reflect.enum.new("Choice", ["FIRST", "SECOND"]).as_type();
    let map_view = reflect.map.new(choice, reflect.Type.of<int>()).as_type().as_map() ?? throw "expected map";
    let key_enum = map_view.key_type().as_enum() ?? throw "expected key enum";
    assert.equal((key_enum.values().at(1) ?? throw "key rows").name, "SECOND")
}

test "literal_views_return_values_for_all_public_literal_kinds" {
    let string_view = reflect.literal.new("line\nquoted \"text\"");
    let int_view = reflect.Type.of<-42>().as_literal() ?? throw "int literal";
    let bigint_view = reflect.Type.of<123456789012345678901234567890n>().as_literal() ?? throw "bigint literal";
    let bool_view = reflect.Type.of<true>().as_literal() ?? throw "bool literal";

    assert.equal(string_view.value(), "line\nquoted \"text\"");
    assert.equal((reflect.Type.of<"fixed">().as_literal() ?? throw "static string literal").value(), "fixed");
    assert.equal(int_view.value(), -42);
    assert.equal(bigint_view.value(), 123456789012345678901234567890n);
    assert.equal(bool_view.value(), true);
    assert.equal(reflect.literal.new(123456789012345678901234567890n).value(), bigint_view.value())
}

test "nested_class_of_a_runtime_package_type_reads_back" {
    let package = reflect.Package.compile(
        { "schema.baml": `class Leaf { name string } class Root { leaf Leaf }` },
    );
    let root = package.get_class("root.Root") ?? throw "missing Root";
    let field = root.fields().at(0) ?? throw "missing field";
    let field_type = field.type ?? throw "missing field type";
    let leaf = field_type.as_class() ?? throw "expected a class view";
    let leaf_field = leaf.fields().at(0) ?? throw "expected a leaf field";
    assert.equal(leaf_field.name, "name")
}

test "function_views_of_a_runtime_package_keep_its_definitions" {
    let package = reflect.Package.compile(
        { "schema.baml": `enum Choice { FIRST SECOND } function pick(c: Choice) -> Choice { c }` },
    );

    let view = package.functions().get("root.pick") ?? throw "missing pick";
    let returned = view.return_type().as_enum() ?? throw "expected a return enum";
    let returned_row = returned.values().at(0) ?? throw "no return rows";
    let param = view.params().at(0) ?? throw "expected a param";
    let param_enum = param.type.as_enum() ?? throw "expected a param enum";
    let param_row = param_enum.values().at(1) ?? throw "no param rows";

    let callable = package.get_function<reflect.AnyFunction<Returns = unknown, Throws = unknown>>("root.pick")
        ?? throw "missing callable";
    let sig = reflect.signature(callable);
    let sig_returns = sig.returns.as_enum() ?? throw "expected a signature return enum";
    let sig_return_row = sig_returns.values().at(0) ?? throw "no signature return rows";
    let arg = sig.args.at(0) ?? throw "expected a signature arg";
    let sig_arg = arg.type.as_enum() ?? throw "expected a signature arg enum";
    let sig_arg_row = sig_arg.values().at(1) ?? throw "no signature arg rows";

    assert.equal(returned_row.name, "FIRST");
    assert.equal(param_row.name, "SECOND");
    assert.equal(sig_return_row.name, "FIRST");
    assert.equal(sig_arg_row.name, "SECOND")
}
