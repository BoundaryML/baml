function main() -> string {
    let inner = reflect.class.builder("Inner");
    inner.field("value", reflect.Type.of<string>());
    let pending = inner.type();
    inner.build();

    let outer = reflect.class.builder("Outer");
    outer.field("nested", pending.meta(alias = "kid", description = "Frozen child"));
    let built = outer.build().as_type().as_class() ?? throw "expected class";
    let field = built.fields().at(0) ?? throw "expected field";
    `${field.name}|${field.meta.alias ?? "none"}|${field.meta.description ?? "none"}`
}

test "metadata_survives_an_already_resolved_pending_reference" {
    assert.equal(main(), "nested|kid|Frozen child")
}
