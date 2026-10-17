function main() -> string {
    let frozen = reflect.Class.builder("FrozenEmployee");
    frozen.field("name", string.type());
    frozen.build();
    let frozen_error = frozen.field("age", int.type()) catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].message
    };

    // A builder's pending type is not a `let type` operand: the binding is
    // refused when it compiles, not when it runs.
    let session = reflect.Session.new();
    let pending_error = session.eval<int>(
        `let unbuilt = reflect.Class.builder("UnbuiltTenant");
        let type Pending = unbuilt.type();
        1`,
    ) catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].message
    };

    let left = "missing frozen error";
    if (frozen_error is string) {
        left = frozen_error
    }
    let right = "missing pending error";
    if (pending_error is string) {
        right = pending_error
    }
    return left + "|" + right;
}

test "frozen_mutation_and_pending_let_type" {
    let result = main();
    assert.contains(result, "`FrozenEmployee` is frozen");
    assert.contains(result, "cannot bind a class builder's pending type; call `build()`")
}
