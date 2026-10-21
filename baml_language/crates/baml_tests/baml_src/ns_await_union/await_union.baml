// BEP-034: `await` distributes over unions of futures.
//
// `Future` is invariant, so combining differently-typed futures (if/else, array
// elements) yields `Future<A, E1> | Future<B, E2>`; awaiting it gives value
// `A | B` and error `E1 | E2`.
//
// Converted from crates/bex_engine/tests/await_union.rs. Each Rust test's
// `function main()` becomes a top-level helper named after the test — the VM
// boxes `test`-block locals (a local compares unequal to a literal), so every
// value under test is produced by a function call and asserted on directly. The
// two Rust sources each declared a `pick`; they are renamed apart here
// (`pick_value` / `pick_error`) since both now live in one namespace.

// ─── await_union_of_futures_value_side ────────────────────────────────────────

function as_int() -> int { 41 }
function as_string() -> string { "x" }

function pick_value(use_int: bool) -> int | string {
    let f = if (use_int) { spawn { as_int() } } else { spawn { as_string() } };
    await f
}

// The BEP's motivating shape: an if/else picking between differently-typed
// futures. The await's value type is the union of the members' value types.
function await_union_of_futures_value_side() -> int {
    let v = pick_value(true);
    match (v) {
        let n: int => n + 1,
        _ => -1
    }
}

test "await_union_of_futures_value_side" {
    assert.equal(await_union_of_futures_value_side(), 42)
}

// ─── await_union_of_futures_error_side ────────────────────────────────────────

function bad_a() -> int throws string { throw "a" }
function bad_b() -> int throws int { throw 7 }

// Error side: each member's `E` is part of the await's escaping throws, so a
// `throws never` contract on the awaiting fn is violated by EITHER error type —
// and an undeclared fn forwards the union.
function pick_error(use_a: bool) -> int {
    let f = if (use_a) { spawn { bad_a() } } else { spawn { bad_b() } };
    (await f) catch (e) {
        let s: string => 1,
        let n: int => n
    }
}

function await_union_of_futures_error_side() -> int {
    pick_error(false)
}

test "await_union_of_futures_error_side" {
    assert.equal(await_union_of_futures_error_side(), 7)
}
