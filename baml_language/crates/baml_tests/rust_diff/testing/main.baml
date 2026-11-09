class TestError { message: string, code: int }
enum TestMarker { One, Two }

function throw_error() -> int {
    throw TestError { message: "test error", code: 7 }
}

function rethrow_error() -> int {
    throw_error() catch (e) { baml.errors.InvalidArgument => 0 }
}

function bad_index() -> int {
    let xs = [1];
    xs[3]
}

test "all assertions pass" {
    assert.is_true(true);
    assert.not_null(7);
    assert.equal([1, 2], [1, 2]);
    assert.equal(TestError { message: "same", code: 3 }, TestError { message: "same", code: 3 });
    assert.equal(TestMarker.One, TestMarker.One);
    assert.approx_equal(1.0, 1.0001, 0.001);
    assert.contains("héllo", "é");
    assert.equal(assert.is_type<int>(7), 7);
    assert.equal(assert.is_type<TestError>(TestError { message: "type", code: 3 }).code, 3);
    let n = throw_error() catch (e, ctx) { let f: TestError => {
        assert.equal(ctx.error, f);
        assert.equal(ctx.root_cause().error, f);
        f.code
    } };
    assert.equal(n, 7)
}

test "is_true failure" { assert.is_true(false) }
test "not_null failure" { assert.not_null(null) }
test "equal failure" { assert.equal("one", "two") }
test "approx delta failure" { assert.approx_equal(1.0, 3.0, 0.01) }
test "approx epsilon failure" { assert.approx_equal(1.0, 1.0, -0.1) }
test "contains failure" { assert.contains("hello", "absent") }
test "is_type failure" { assert.is_type<TestError>("wrong"); }
test "panic failure" { baml.sys.panic("test panic") }
test "typed failure" { throw_error(); }
test "rethrow failure" { rethrow_error(); }
test "scalar failure" { throw "scalar error" }
test "enum failure" { throw TestMarker.Two }
test "runtime panic failure" { assert.equal(bad_index(), 0) }
test "cause chain failure" {
    throw_error() catch (e, ctx) { TestError => {
        assert.equal(ctx.cause, null);
        throw baml.errors.InvalidArgument { message: "second error" }
    } };
}

testset "nested discovery" {
    test "passes" { assert.equal(2, 2) }
}
