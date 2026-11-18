// A diverging `||` right operand, `??` fallback, `?.` argument or catch arm
// may not run, so it does not end the flow around it: the write after it is
// seen after the loop. A write inside one holds on one path only. The compile
// errors for the shapes this rejects are in the diagnostics project
// `diagnostic_errors/divergence_scoping`; `LnTag` is declared in the sibling
// file `loop_narrowing.baml`.
//
// Every case body is a top-level function; a `test` block only asserts.

class LnBox {
    function m(self, v: int) -> int throws never {
        v
    }
}

function ln_thrower(a: int) -> int throws string {
    if (a > 0) {
        throw "boom";
    }
    a
}

// A diverging `||` right operand or catch arm may not run: the write after
// it is seen after the loop.
function ln_or_divergence_is_scoped(xs: int[]) -> string {
    let x: int | LnTag = 0;
    for let v in xs {
        let ok = v < 5 || { return "unreachable"; };
        x = LnTag { name: "tag" };
    }
    match (x) {
        int => "int",
        let t: LnTag => t.name,
    }
}

function ln_catch_divergence_is_scoped(xs: int[]) -> string {
    let x: int | LnTag = 0;
    for let v in xs {
        let r = ln_thrower(v) catch (e) { _ => { return "caught"; } };
        x = LnTag { name: "tag" };
    }
    match (x) {
        int => "int",
        let t: LnTag => t.name,
    }
}

// A write inside a skipped operand holds exactly when the operand ran.
function ln_null_coalesce_fallback_write_joins(o: int?) -> string {
    let x: int | LnTag = 0;
    let r = o ?? { x = LnTag { name: "fallback" }; 0 };
    match (x) {
        int => "int",
        let t: LnTag => t.name,
    }
}

function ln_optional_call_argument_write_joins(b: LnBox?) -> string {
    let x: int | LnTag = 0;
    let r = b?.m({ x = LnTag { name: "argument" }; 1 });
    match (x) {
        int => "int",
        let t: LnTag => t.name,
    }
}

// A catch whose base and every arm diverge is the end of the flow.
function ln_catch_with_everything_diverging() -> int {
    let r = { throw "boom"; } catch (e) { _ => { return 2; } };
}

test "a diverging right operand or catch arm does not end the body's flow" {
    assert.equal(ln_or_divergence_is_scoped([1]), "tag");
    assert.equal(ln_catch_divergence_is_scoped([0]), "tag");
    assert.equal(ln_catch_divergence_is_scoped([1]), "caught");
    assert.equal(ln_catch_with_everything_diverging(), 2);
}

test "a write inside a skipped operand holds only when the operand ran" {
    assert.equal(ln_null_coalesce_fallback_write_joins(1), "int");
    assert.equal(ln_null_coalesce_fallback_write_joins(null), "fallback");
    assert.equal(ln_optional_call_argument_write_joins(null), "int");
    assert.equal(ln_optional_call_argument_write_joins(LnBox {}), "argument");
}
