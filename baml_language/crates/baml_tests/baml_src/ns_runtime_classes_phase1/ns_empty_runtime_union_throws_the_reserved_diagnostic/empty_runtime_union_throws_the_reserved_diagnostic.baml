function main() -> string {
    let result = reflect.union.new([]) catch (e) {
        reflect.errors.CompilationError => {
            e.diagnostics[0].code + "|" + e.diagnostics[0].message
        }
    };
    if (result is string) {
        return result;
    }
    return "constructor did not throw";
}

test "empty_runtime_union_throws_the_reserved_diagnostic" {
    assert.equal(main(), "E0160|a runtime union must contain at least one member")
}
