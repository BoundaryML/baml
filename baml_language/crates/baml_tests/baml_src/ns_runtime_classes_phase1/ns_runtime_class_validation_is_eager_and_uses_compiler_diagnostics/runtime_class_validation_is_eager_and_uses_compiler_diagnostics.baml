function main() -> string throws never {
    let bad_name = reflect.Class.new("bad name", {}) catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code
    };
    let duplicate_wire_key = reflect.Class.new(
        "Collision",
        {
            "wire": string.type(),
            "internal": int.type().meta(alias = "wire"),
        },
    ) catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code
    };
    let name_code = "name did not throw";
    if (bad_name is string) {
        name_code = bad_name
    }
    let key_code = "key did not throw";
    if (duplicate_wire_key is string) {
        key_code = duplicate_wire_key
    }
    return name_code + "|" + key_code;
}

test "runtime_class_validation_is_eager_and_uses_compiler_diagnostics" {
    assert.equal(main(), "E0010|E0149")
}
