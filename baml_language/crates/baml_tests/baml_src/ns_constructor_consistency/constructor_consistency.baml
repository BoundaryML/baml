test "reserved words are rejected as reflection constructor names" {
    let class_name = reflect.Class.new("string", {}) catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
    };
    let field_name = reflect.Class.new("ValidClass", { "self": string.type() }) catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
    };
    let enum_name = reflect.Enum.new("class", []) catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
    };
    let variant_name = reflect.Enum.new("ValidEnum", ["function"]) catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
    };

    let class_result = if (class_name is string) {
        class_name
    } else {
        "class name accepted"
    };
    let field_result = if (field_name is string) {
        field_name
    } else {
        "field name accepted"
    };
    let enum_result = if (enum_name is string) {
        enum_name
    } else {
        "enum name accepted"
    };
    let variant_result = if (variant_name is string) {
        variant_name
    } else {
        "variant name accepted"
    };
    assert.equal(
        class_result + "\n" + field_result + "\n" + enum_result + "\n" + variant_result,
        "E0010|invalid class name `string`\nE0010|invalid field name `ValidClass.self`\nE0010|invalid enum name `class`\nE0010|invalid enum variant name `ValidEnum.function`",
    )
}

function make_enum() -> reflect.Enum {
    reflect.Enum.new("PreciseEnum", [])
}

function accept_runtime_type<T>() -> string {
    "ok"
}

function use_precise_enum_as_runtime_type() -> string {
    let enum_t = reflect.Enum.new("RuntimeEnum", []);
    let type E = enum_t;
    accept_runtime_type<E>()
}

test "enum constructor has its precise declared kind" {
    let enum_type: reflect.Enum = make_enum();
    assert.is_true(enum_type is reflect.Enum);
    assert.equal(use_precise_enum_as_runtime_type(), "ok")
}
