// A written lambda annotation commits into the callee's expected function
// type (rustc's check_supplied_sig_against_expectation): the generic call's
// type argument is "known unambiguously at the call site" (TYPE_SYSTEM.md).
//
// Regression net for the post-#4301 miscompile class: without the commit,
// `Array.map`'s `U` stayed an unsolved var during the walk, a later member
// access on a `U`-derived receiver silently finalized `Error`, and MIR
// lowered the call down the wrong dispatch path — a well-typed program that
// died with `VM internal error: expected map, got array` (or, under a spawn,
// an `Error`-reaches-`RuntimeTy` lowering ICE).

test "map-produced futures joined by future.all, un-ascribed binding" {
    let futures = [1, 2, 3].map((n: int) -> baml.future.Future<string, never> {
        spawn { "x-" + n.to_string() }
    });
    let all = (await baml.future.all(futures)) catch_all (e) { _ => ["failed"] };
    assert.equal(all.join(","), "x-1,x-2,x-3")
}

test "same shape under an outer spawn" {
    let parent = spawn {
        let futures = [1, 2, 3].map((n: int) -> baml.future.Future<string, never> {
            spawn { "y-" + n.to_string() }
        });
        let all = (await baml.future.all(futures)) catch_all (e) { _ => ["failed"] };
        all.join(",")
    };
    let joined = (await parent) catch_all (e) { _ => "spawn-failed" };
    assert.equal(joined, "y-1,y-2,y-3")
}

test "annotation deduces through a nested constructor slot" {
    // expected ret is `?U[]` — the var sits INSIDE the constructor, so the
    // commit must unify through it, not only when the whole slot is a var.
    let doubled = [1, 2, 3].map((n: int) -> int[] { [n, n * 2] });
    assert.equal(doubled.length(), 3);
    assert.equal(doubled[2][1], 6)
}

test "stored lambda parameter is inferred from a later annotation" {
    let identity = (value) -> {
        value
    };
    let constrained: (int) -> int throws never = identity;
    assert.equal(constrained(42), 42)
}
