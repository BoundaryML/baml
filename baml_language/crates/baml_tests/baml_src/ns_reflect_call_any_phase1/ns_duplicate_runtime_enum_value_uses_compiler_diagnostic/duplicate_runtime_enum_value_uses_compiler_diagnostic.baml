function main() -> string throws never {
    let result = reflect.enum.new("Category", ["RED", "RED"]) catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message
    };
    if (result is string) {
        return result;
    }
    return "constructor did not throw";
}

test "duplicate_runtime_enum_value_uses_compiler_diagnostic" {
    let result = main();
    assert.is_true(result.starts_with("E0012|"));
    assert.contains(result, "duplicate variant `Category.RED`")
}
