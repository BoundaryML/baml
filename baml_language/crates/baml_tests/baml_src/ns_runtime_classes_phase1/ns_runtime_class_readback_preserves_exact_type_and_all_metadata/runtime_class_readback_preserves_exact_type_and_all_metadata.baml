function main() -> bool {
    let literal = reflect.literal.new("fixed");
    let schema = reflect.class.new(
        "Schema",
        {
            "tag": literal.as_type().meta(
                alias = "wire_tag",
                description = "dispatch key",
                docstring = "source docs",
                other = { "owner": "slice3" },
            ),
        },
    );
    let field = schema.fields()[0];
    field.name == "tag"
        && field.type == literal.as_type()
        && field.meta.alias == "wire_tag"
        && field.meta.description == "dispatch key"
        && field.meta.docstring == "source docs"
        && field.meta.other.get("owner") == "slice3"
}

test "runtime_class_readback_preserves_exact_type_and_all_metadata" {
    assert.equal(main(), true)
}
