function main() -> string throws never {
    // Manual Signature construction through the plain class literal.
    // (No keyword-named fields: the error channel field is `errors`.)
    let manual = reflect.Signature {
        args: [],
        opts: {},
        returns: reflect.Type.of<int>(),
        errors: reflect.Type.of<never>(),
        docstring: null,
    };
    return manual.returns.to_string() + "|" + manual.errors.to_string();
}

test "signature_object_literal_construction" {
    assert.equal(main(), "int|never")
}
