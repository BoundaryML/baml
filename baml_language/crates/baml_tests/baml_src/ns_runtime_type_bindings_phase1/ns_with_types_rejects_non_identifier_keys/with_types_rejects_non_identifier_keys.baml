function main() -> string {
    let dynamic_t = reflect.class.new("Dynamic", { "value": reflect.Type.of<string>() });
    let base = reflect.Package.compile({ "empty.baml": "" });
    let result = base.with_types({ "not an identifier": dynamic_t }) catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code
    };
    if (result is string) {
        result
    } else {
        "invalid key was accepted"
    }
}

test "with_types_rejects_non_identifier_keys" {
    assert.equal(main(), "E0010")
}
