function main() -> string {
    let builder = reflect.class.builder("Duplicate");
    builder.field("name", reflect.Type.of<string>());
    let result = builder.field("name", reflect.Type.of<int>()) catch (e) {
        reflect.errors.CompilationError => {
            let span = e.diagnostics[0].span;
            return e.diagnostics[0].code + "|" + (span == null).to_string();
        }
    };
    return "duplicate did not throw";
}

test "duplicate_field_structured_diagnostic_has_a_null_span" {
    assert.equal(main(), "E0012|true")
}
