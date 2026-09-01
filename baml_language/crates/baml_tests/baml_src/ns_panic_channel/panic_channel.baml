// Panics are RAISED, not thrown: a `baml.panics.*` value travels the ordinary
// unwind path but is excluded from three places — the `throws` contract, what a
// `_` catch arm traps, and `catch_all` exhaustiveness.
//
// Classification is purely the namespace (`baml.panics`, checked by
// `is_panic_namespace`), so `throw baml.panics.X` raises exactly as
// `baml.sys.panic` does and contributes nothing to the channel.

// A `throw` of a panic class leaves the clause `never` — it raised, it did not
// throw. Declaring it would be extraneous (E0097).
function throws_a_panic() -> int throws never {
    throw baml.panics.AssertionFailed { message: "raised, not thrown" };
}

// Mixing a panic with an ordinary error contributes only the ordinary half.
function throws_a_mixture(fail: bool) -> int throws baml.errors.InvalidArgument {
    if (fail) {
        throw baml.panics.AssertionFailed { message: "raised" };
    } else {
        throw baml.errors.InvalidArgument { message: "thrown" };
    }
}

test "a thrown panic is caught by an arm that names it" {
    let caught = throws_a_panic() catch (e) {
        baml.panics.AssertionFailed => 1,
    };
    assert.equal(caught, 1);
}

test "a wildcard arm does not swallow a thrown panic" {
    // `_` skips panics, so the inner catch re-raises and the named outer arm
    // is what actually traps it.
    let caught = {
        throws_a_panic() catch (inner) {
            _ => 1,
        }
    } catch (e) {
        baml.panics.AssertionFailed => 2,
    };
    assert.equal(caught, 2);
}

test "catch_all does not swallow a thrown panic either" {
    let caught = {
        throws_a_panic() catch_all (inner) {
            _ => 1,
        }
    } catch (e) {
        baml.panics.AssertionFailed => 2,
    };
    assert.equal(caught, 2);
}

test "the ordinary half of a mixed throw still reaches the clause" {
    let caught = throws_a_mixture(false) catch (e) {
        baml.errors.InvalidArgument => 3,
    };
    assert.equal(caught, 3);
}

test "the panic half of a mixed throw still raises" {
    let caught = throws_a_mixture(true) catch (e) {
        baml.panics.AssertionFailed => 4,
    };
    assert.equal(caught, 4);
}
