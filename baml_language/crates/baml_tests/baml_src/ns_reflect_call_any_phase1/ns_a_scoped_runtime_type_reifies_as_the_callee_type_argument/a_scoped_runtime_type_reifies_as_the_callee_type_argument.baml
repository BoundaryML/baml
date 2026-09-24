function inspect<T>() -> string throws never {
    return reflect.Type.of<T>().to_string();
}

function main() -> string throws reflect.errors.CompilationError {
    let t = reflect.enum.new("Category", ["RED", "BLUE"]);
    type T = unreflect(t)
    return inspect<T>();
}

test "a_scoped_runtime_type_reifies_as_the_callee_type_argument" {
    assert.equal(main(), "Category")
}
