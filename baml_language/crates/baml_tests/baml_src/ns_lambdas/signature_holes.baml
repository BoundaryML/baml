// MIR must consume the checked signature, not re-lower written inference holes.
test "lambda_return_hole_matches_omitted_and_explicit" {
    let hole = () -> _ { 1 };
    let omitted = () -> { 1 };
    let explicit = () -> int { 1 };
    assert.equal(hole(), 1);
    assert.equal(hole(), omitted());
    assert.equal(hole(), explicit());
    assert.equal(reflect.signature(hole).returns.to_string(), "int");
    assert.equal(reflect.signature(hole).errors.to_string(), "never")
}

test "lambda_nested_return_hole" {
    let values = () -> map<string, _> { { "a": 1 } };
    assert.equal(values().get("a"), 1);
    assert.is_true(reflect.signature(values).returns == reflect.Type.of<map<string, int>>())
}

test "lambda_parameter_and_return_from_context" {
    let identity: (int) -> int throws never = (x) -> _ { x };
    assert.equal(identity(7), 7);
    assert.is_true(identity is (int) -> int throws never);
    assert.equal(reflect.signature(identity).returns.to_string(), "int")
}

test "lambda_throws_hole" {
    let fail = () -> int throws _ { throw "failed" };
    assert.equal(reflect.signature(fail).errors.to_string(), "string");
    let result = fail() catch (e) { string => 42 };
    assert.equal(result, 42)
}

function signature_holes_generic<T>(value: T) -> string {
    let identity = (x: T) -> _ { x };
    assert.equal(identity(value), value);
    reflect.signature(identity).returns.to_string()
}

test "lambda_hole_preserves_enclosing_generic_frame" {
    assert.equal(signature_holes_generic<int>(9), "int");
    assert.equal(signature_holes_generic<string>("nine"), "string")
}

test "lambda_explicit_unknown_is_not_an_unsolved_hole" {
    let opaque = (x: unknown) -> unknown { x };
    assert.equal(reflect.signature(opaque).returns.to_string(), "unknown");
    assert.is_true(opaque(1) is int)
}
