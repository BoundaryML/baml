// Control-flow interplay: match-arm break/continue/return inside loops, if-let
// expressions and statements, and let-else bindings.
//
// Converted from crates/bex_engine/tests/control_flow.rs. Each Rust test's
// `function main()` body keeps its helper function(s); the test block calls the
// helper with the literal argument and compares the result. Helper functions
// that originally shared a name across separate Rust sources are given distinct
// names here since one namespace can't declare the same function twice.

// ─── shared declarations ──────────────────────────────────────────────────────

// A shared member accessed on a union (`(Ok | Warn).value`) is reachable only
// through an interface both arms implement — a bare class field is not enough.
interface HasValue { value string }

class Ok { value string  implements HasValue {} }
class Err { message string }
class Empty {}
class Warn { value string  implements HasValue {} }
class User { name string, age int }
class Admin { handle string }
class Wrapper { payload string }
class Other {}
class Item { name string }
class NoMatch {}

// ─── match-arm control transfer inside loops ──────────────────────────────────

function break_loop(x: int) -> int {
    let result = 0;
    while (true) {
        match (x) {
            1 => {
                result = 40;
                break;
            },
            _ => {
                result = 2;
                break;
            },
        };
    }

    result
}

function continue_loop(x: int) -> int {
    let i = 0;
    while (i < 3) {
        match (x) {
            1 => {
                i += 1;
                continue;
            },
            _ => {
                i += 10;
                continue;
            },
        };

        i = 100;
    }

    i
}

function return_from_match(x: int) -> int {
    match (x) {
        1 => {
            return 40;
        },
        _ => {
            return 2;
        },
    };

    999
}

test "match_arm_break_exits_infinite_while_loop" {
    assert.equal(break_loop(1) + break_loop(2), 42)
}

test "match_arm_continue_continues_loop_without_falling_through" {
    assert.equal(continue_loop(1) + continue_loop(2), 13)
}

test "match_arm_return_returns_without_falling_through" {
    assert.equal(return_from_match(1) + return_from_match(2), 42)
}

// ─── if-let ───────────────────────────────────────────────────────────────────

function describe_then_default(r: Ok | Err) -> string {
    if let o: Ok = r {
        o.value
    } else {
        "no match"
    }
}

function describe_else_message(r: Ok | Err) -> string {
    if let o: Ok = r {
        o.value
    } else {
        r.message
    }
}

function greet_if_let(u: User | Admin) -> string {
    if let User { name, age } = u {
        name
    } else {
        "admin"
    }
}

function pick_ok_err_empty(r: Ok | Err | Empty) -> string {
    if let o: Ok = r {
        o.value
    } else if let e: Err = r {
        e.message
    } else {
        "empty"
    }
}

function describe_or_pattern(r: Ok | Warn | Err) -> string {
    if let s: Ok | Warn = r {
        s.value
    } else {
        r.message
    }
}

function pick_nested(outer: Wrapper, inner: Wrapper | Other) -> string {
    if let w: Wrapper = outer {
        if let w: Wrapper = inner {
            w.payload
        } else {
            w.payload
        }
    } else {
        "none"
    }
}

function maybe_ok() -> Ok | Err {
    Ok { value: "from call" }
}

function if_let_call() -> string {
    if let o: Ok = maybe_ok() {
        o.value
    } else {
        "no match"
    }
}

function pick_optional(item: Item?) -> string {
    if let i: Item = item {
        i.name
    } else {
        "none"
    }
}

function first_value_statement(r: Ok | Err) -> string {
    if let o: Ok = r {
        return o.value;
    }
    "fell through"
}

test "if_let_takes_then_branch_when_pattern_matches" {
    assert.equal(describe_then_default(Ok { value: "hit" }), "hit")
}

test "if_let_takes_else_branch_when_pattern_does_not_match" {
    assert.equal(describe_else_message(Err { message: "boom" }), "boom")
}

test "if_let_destructure_binds_fields_at_runtime" {
    assert.equal(greet_if_let(User { name: "alice", age: 30 }), "alice")
}

test "if_let_else_if_let_chain_runs_correct_arm" {
    assert.equal(pick_ok_err_empty(Err { message: "second" }), "second")
}

test "if_let_chain_falls_through_to_final_else" {
    assert.equal(pick_ok_err_empty(Empty {}), "empty")
}

test "if_let_or_pattern_dispatches_to_either_alternative" {
    assert.equal(describe_or_pattern(Ok { value: "ok!" }), "ok!");
    assert.equal(describe_or_pattern(Warn { value: "warn!" }), "warn!");
    assert.equal(describe_or_pattern(Err { message: "boom" }), "boom")
}

test "if_let_nested_shadowing_restores_outer_binding" {
    assert.equal(pick_nested(Wrapper { payload: "outer" }, Other {}), "outer")
}

test "if_let_composite_scrutinee_binds_call_result" {
    assert.equal(if_let_call(), "from call")
}

test "if_let_optional_scrutinee_some" {
    assert.equal(pick_optional(Item { name: "alice" }), "alice")
}

test "if_let_optional_scrutinee_null" {
    assert.equal(pick_optional(null), "none")
}

test "if_let_statement_form_with_return_in_then" {
    assert.equal(first_value_statement(Err { message: "ignored" }), "fell through")
}

// ─── let-else ───────────────────────────────────────────────────────────────

function get_value_or_fallback(r: Ok | Err) -> string {
    let o: Ok = r else { return "fallback"; };
    o.value
}

function greet_let_else(u: User | Admin) -> string {
    let User { name, age } = u else { return "admin"; };
    name
}

function pick_let_else(r: Ok | Warn | Err) -> string {
    let s: Ok | Warn = r else { return "err"; };
    s.value
}

function pick_first_ok(items: (Ok | Err)[]) -> string {
    let result = "no-ok";
    let i = 0;
    while (i < 3) {
        let item = items[i];
        let o: Ok = item else { break; };
        result = o.value;
        i += 1;
    }
    result
}

function concat_oks(items: (Ok | Err)[]) -> string {
    let acc = "";
    let i = 0;
    while (i < 3) {
        let item = items[i];
        i += 1;
        let o: Ok = item else { continue; };
        acc = acc + o.value;
    }
    acc
}

function chain_two(a: Ok | Err, b: Ok | Err) -> string {
    let oa: Ok = a else { return "a-failed"; };
    let ob: Ok = b else { return "b-failed"; };
    oa.value + "-" + ob.value
}

function inner_throws(r: Ok | Err) -> string throws NoMatch {
    let o: Ok = r else { throw NoMatch {}; };
    o.value
}

function main_caught() -> string {
    inner_throws(Err { message: "ignored" }) catch (e) {
        NoMatch => "caught"
    }
}

test "let_else_binds_when_pattern_matches" {
    assert.equal(get_value_or_fallback(Ok { value: "hit" }), "hit")
}

test "let_else_takes_else_branch_when_pattern_does_not_match" {
    assert.equal(get_value_or_fallback(Err { message: "ignored" }), "fallback")
}

test "let_else_destructure_binds_fields_at_runtime" {
    assert.equal(greet_let_else(User { name: "alice", age: 30 }), "alice")
}

test "let_else_or_pattern_matches_either_alternative" {
    assert.equal(pick_let_else(Warn { value: "warn-val" }), "warn-val")
}

test "let_else_break_in_else_inside_loop" {
    assert.equal(
        pick_first_ok([Ok { value: "first" }, Err { message: "stop" }, Ok { value: "third" }]),
        "first",
    )
}

test "let_else_continue_in_else_inside_loop" {
    assert.equal(concat_oks([Ok { value: "a" }, Err { message: "skip" }, Ok { value: "c" }]), "ac")
}

test "let_else_multiple_consecutive_bindings" {
    assert.equal(chain_two(Ok { value: "x" }, Ok { value: "y" }), "x-y")
}

test "let_else_second_miss_skips_first_binding" {
    assert.equal(chain_two(Ok { value: "x" }, Err { message: "boom" }), "b-failed")
}

test "let_else_throw_in_else_is_caught_by_outer_catch" {
    assert.equal(main_caught(), "caught")
}

test "let_else_throw_in_else_propagates_when_uncaught" {
    {
        inner_throws(Err { message: "boom" });
        baml.sys.panic("expected NoMatch throw")
    } catch (e) {
        NoMatch => null
    }
}
