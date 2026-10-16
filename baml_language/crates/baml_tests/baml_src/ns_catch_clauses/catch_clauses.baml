// `catch`, `catch_all`, and `catch_all_panics` share one dispatch: every arm
// of every clause in a chain is tried in source order, and a clause's kind
// only decides whether its wildcard arms (`_`, a bare `let x`) receive
// panics. A panic passes over a wildcard arm whose clause does not trap
// panics and is offered to the arms after it, so a later clause in the same
// chain can still catch it.

class NotFound {
    key: string,
}

function cc_lookup(n: int) -> int {
    match (n) {
        0 => throw NotFound { key: "k" },
        1 => throw "oops",
        _ => 42,
    }
}

function cc_index_panic() -> int {
    let xs = [1];
    xs[5]
}

// A `catch` wildcard before a `catch_all_panics` clause: the panic passes the
// first clause's `_` and reaches the second clause.
function cc_wildcard_then_catch_all_panics() -> string {
    cc_index_panic().to_string() catch (e) {
        _ => "first clause",
    } catch_all_panics (p) {
        _ => "panic clause",
    }
}

test "a panic passes a catch wildcard and reaches a later catch_all_panics clause" {
    assert.equal(cc_wildcard_then_catch_all_panics(), "panic clause")
}

// An error is still taken by the first clause's wildcard.
function cc_error_wildcard_then_catch_all_panics() -> string {
    cc_lookup(0).to_string() catch (e) {
        _ => "first clause",
    } catch_all_panics (p) {
        _ => "panic clause",
    }
}

test "an error is taken by the first clause's wildcard" {
    assert.equal(cc_error_wildcard_then_catch_all_panics(), "first clause")
}

// A panic passes a wildcard and reaches a named panic arm in a later clause.
function cc_wildcard_then_named_panic() -> string {
    cc_index_panic().to_string() catch (e) {
        _ => "wildcard",
    } catch (p) {
        baml.panics.IndexOutOfBounds => "named panic",
    }
}

test "a panic passes a wildcard and reaches a later named panic arm" {
    assert.equal(cc_wildcard_then_named_panic(), "named panic")
}

// A panic no arm takes still propagates past the whole chain.
function cc_unmatched_panic_propagates() -> string {
    {
        cc_index_panic().to_string() catch (e) {
            _ => "wildcard",
        } catch_all (e2) {
            _ => "catch_all wildcard",
        }
    } catch (outer) {
        baml.panics.IndexOutOfBounds => "outer",
    }
}

test "a panic no clause traps propagates past the chain" {
    assert.equal(cc_unmatched_panic_propagates(), "outer")
}

// `catch_all` covers what reaches it, so nothing propagates and a
// `throws never` contract holds.
function cc_catch_all_covers(n: int) -> int throws never {
    cc_lookup(n) catch_all (e) {
        NotFound => 1,
        string => 2,
    }
}

test "catch_all handles every error that reaches it" {
    assert.equal(cc_catch_all_covers(0) + cc_catch_all_covers(1) + cc_catch_all_covers(2), 45)
}

// A `catch_all` after a partial `catch` covers only the residual.
function cc_catch_then_catch_all(n: int) -> int throws never {
    cc_lookup(n) catch (e) {
        NotFound => 1,
    } catch_all (e2) {
        string => 2,
    }
}

test "catch_all after a partial catch covers the residual" {
    assert.equal(cc_catch_then_catch_all(0) + cc_catch_then_catch_all(1), 3)
}

// `catch_all_panics` covering its panics with a named alias arm.
function cc_catch_all_panics_named(n: int) -> string throws never {
    let r = {
        cc_lookup(n);
        cc_index_panic()
    } catch_all_panics (e) {
        NotFound => "not found",
        string => "string",
        baml.panics.Panic => "panic",
    };
    r.to_string()
}

test "catch_all_panics with a named panic arm" {
    assert.equal(cc_catch_all_panics_named(0), "not found");
    assert.equal(cc_catch_all_panics_named(1), "string");
    assert.equal(cc_catch_all_panics_named(2), "panic")
}

// A bare name or literal as the protected expression.
function cc_bare_name_base() -> int {
    let x = 1;
    x catch (e) {
        _ => 2,
    }
}

function cc_bare_literal_operand() -> int {
    1 + 1 catch (e) {
        _ => 2,
    }
}

test "a bare name or literal can be the protected expression" {
    assert.equal(cc_bare_name_base(), 1);
    assert.equal(cc_bare_literal_operand(), 2)
}

// A `catch_all_panics` wildcard receives panics even after every error is
// handled, so its binding is typed as a panic, not `never`.
function cc_wildcard_binding_is_a_panic() -> string {
    cc_lookup(5).to_string() catch_all_panics (e) {
        _ => "unused",
    };
    cc_index_panic().to_string() catch_all_panics (e) {
        let p => match (p) {
            let oob: baml.panics.IndexOutOfBounds => "index " + oob.index.to_string(),
            _ => "other panic",
        },
    }
}

test "a catch_all_panics wildcard binding receives the panic" {
    assert.equal(cc_wildcard_binding_is_a_panic(), "index 5")
}
