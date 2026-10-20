function main() -> string {
    let builder = reflect.Class.builder("Duplicate");
    builder.field("name", string.type());
    let result = builder.field("name", int.type()) catch (e) {
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
