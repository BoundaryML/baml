test "reserved words are rejected as reflection constructor names" {
    let class_name = reflect.class.new("type", {}) catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
    };
    let field_name = reflect.class.new("ValidClass", { "test": reflect.Type.of<string>() }) catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
    };
    let enum_name = reflect.enum.new("class", []) catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
    };
    let variant_name = reflect.enum.new("ValidEnum", ["function"]) catch (e) {
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
        "E0010|invalid class name `type`\nE0010|invalid field name `ValidClass.test`\nE0010|invalid enum name `class`\nE0010|invalid enum variant name `ValidEnum.function`",
    )
}

function make_enum() -> reflect.enum.Type {
    reflect.enum.new("PreciseEnum", [])
}

function accept_runtime_type<T>() -> string {
    "ok"
}

function use_precise_enum_as_runtime_type() -> string {
    let enum_t = reflect.enum.new("RuntimeEnum", []);
    type E = unreflect(enum_t);
    accept_runtime_type<E>()
}

test "enum constructor has its precise declared kind" {
    let enum_type: reflect.enum.Type = make_enum();
    assert.is_true(enum_type is reflect.enum.Type);
    assert.equal(use_precise_enum_as_runtime_type(), "ok")
}
