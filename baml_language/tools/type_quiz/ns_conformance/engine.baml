// Conformance suite for the engine: determinism of seeds and items (I1) and
// the verifier's own contract, exercised on inline sources so a regression in
// `verify` cannot hide behind a bank item.
//
// Test-block locals are boxed by the VM, so every case body is a function and
// the `test` blocks only assert.

// ─── seeds ───────────────────────────────────────────────────────────────────

function same_state_same_draw() -> bool throws never {
    root.engine.next(root.engine.seed_from(42)) == root.engine.next(root.engine.seed_from(42))
}

/// Every `int` denotes a stream: the state is read modulo 2^32 and zero reads
/// as one, so the multiples of 2^32 and the negatives are all valid.
function every_int_is_a_stream() -> bool throws never {
    let one = root.engine.next(root.engine.Seed { state: 1 });
    root.engine.next(root.engine.Seed { state: 0 }) == one
        && root.engine.next(root.engine.Seed { state: 4294967296 }) == one
        && root.engine.next(root.engine.Seed { state: -4294967296 }) == one
        && root.engine.next(root.engine.Seed { state: -5 }).value >= 1
        && root.engine.next(root.engine.Seed { state: 4294967296 }).value != 0
}

/// Known answers for the hash primitives every seed passes through, so a
/// typo in a magic constant cannot silently change every case in the tool.
function hash_primitives_match_references() -> bool throws never {
    root.engine.mul32(4294967295, 4294967295) == 1
        && root.engine.mul32(65536, 65536) == 0
        && root.engine.mix32(0) == 0
        && root.engine.mix32(1) == 1364076727
        && root.engine.fnv1a32("".to_utf8()) == 2166136261
        && root.engine.fnv1a32("a".to_utf8()) == 3826002220
        && root.engine.fnv1a32("foobar".to_utf8()) == 3214735720
}

/// Neighbouring small seeds must not start neighbouring streams.
function seed_from_mixes() -> bool throws never {
    let a = root.engine.next(root.engine.seed_from(1)).value;
    let b = root.engine.next(root.engine.seed_from(2)).value;
    (a - b).abs() > 1000000
}

function draws_stay_in_range() -> bool throws never {
    let s = root.engine.seed_from(7);
    let i = 0;
    let ok = true;
    while (i < 1000) {
        let d = root.engine.next(s);
        ok = ok && d.value >= 1 && d.value <= root.engine.mask32();
        let b = root.engine.below(s, 13);
        ok = ok && b.value >= 0 && b.value < 13;
        s = d.next;
        i = i + 1;
    }
    ok
}

function stream_does_not_repeat_early() -> bool throws never {
    let s = root.engine.seed_from(99);
    let seen: map<string, bool> = {};
    let i = 0;
    let ok = true;
    while (i < 2000) {
        let d = root.engine.next(s);
        let key = d.value.to_string();
        ok = ok && !seen.has(key);
        let _ = seen.set(key, true);
        s = d.next;
        i = i + 1;
    }
    ok
}

function split_streams_differ() -> bool throws never {
    let pair = root.engine.split(root.engine.seed_from(5));
    root.engine.next(pair.left).value != root.engine.next(pair.right).value
}

function seed_for_is_stable_and_distinct() -> bool throws never {
    let a = root.engine.seed_for("item/a", 0);
    a == root.engine.seed_for("item/a", 0)
        && a != root.engine.seed_for("item/a", 1)
        && a != root.engine.seed_for("item/b", 0)
}

test "seed: same state, same draw" {
    assert.is_true(same_state_same_draw())
}

test "seed: every int denotes a stream" {
    assert.is_true(every_int_is_a_stream())
}

test "seed: hash primitives match their reference values" {
    assert.is_true(hash_primitives_match_references())
}

test "seed: seed_from mixes neighbouring seeds apart" {
    assert.is_true(seed_from_mixes())
}

test "seed: draws stay in range" {
    assert.is_true(draws_stay_in_range())
}

test "seed: stream does not repeat early" {
    assert.is_true(stream_does_not_repeat_early())
}

test "seed: split streams differ" {
    assert.is_true(split_streams_differ())
}

test "seed: seed_for is stable and distinct" {
    assert.is_true(seed_for_is_stable_and_distinct())
}

// ─── verifier ────────────────────────────────────────────────────────────────

function compiling_case(test_body: string) -> root.engine.Case throws never {
    root.engine.Case {
        files: {
            "case.baml": `
                function double(n: int) -> int { n * 2 }
                test "probe" { ${test_body} }
            `,
        },
        key: root.engine.Compiles { tests: ["root::probe"] },
        trace: [],
    }
}

function rejected_case(codes: string[], anchor: string) -> root.engine.Case throws never {
    root.engine.Case {
        files: {
            "case.baml": `
                function mismatch() -> int {
                    let text: int = "s";
                    text
                }
            `,
        },
        key: root.engine.Rejected { codes: codes, anchor: anchor },
        trace: [],
    }
}

function verify_passing() -> root.engine.Outcome throws never {
    root.engine.verify(compiling_case("assert.equal(double(2), 4)"))
}

function verify_failing_test() -> root.engine.Outcome throws never {
    root.engine.verify(compiling_case("assert.equal(double(2), 5)"))
}

function verify_missing_test() -> root.engine.Outcome throws never {
    let case = compiling_case("assert.is_true(true)");
    root.engine.verify(
        root.engine.Case { ...case, key: root.engine.Compiles { tests: ["root::absent"] } },
    )
}

function verify_expected_rejection() -> root.engine.Outcome throws never {
    root.engine.verify(rejected_case(["E0001"], "text"))
}

function verify_wrong_code() -> root.engine.Outcome throws never {
    root.engine.verify(rejected_case(["E9999"], "text"))
}

function verify_wrong_anchor() -> root.engine.Outcome throws never {
    root.engine.verify(rejected_case(["E0001"], "nowhere"))
}

/// A multi-byte character before the offending line: byte offsets and
/// character indices diverge, and the anchor must still be found.
function verify_anchor_after_multibyte() -> root.engine.Outcome throws never {
    root.engine.verify(
        root.engine.Case {
            files: {
                "case.baml": `
                    // 🐑🐑🐑
                    function mismatch() -> int {
                        let text: int = "s";
                        text
                    }
                `,
            },
            key: root.engine.Rejected { codes: ["E0001"], anchor: "text" },
            trace: [],
        },
    )
}

function verify_unexpected_compile() -> root.engine.Outcome throws never {
    let case = compiling_case("assert.is_true(true)");
    root.engine.verify(
        root.engine.Case {
            ...case,
            key: root.engine.Rejected { codes: ["E0001"], anchor: "double" },
        },
    )
}

test "verify: compiling case with passing test agrees" {
    assert.equal(verify_passing(), root.engine.Agrees {  })
}

test "verify: failing test is reported by name" {
    assert.equal(verify_failing_test(), root.engine.FailedTests { names: ["root::probe"] })
}

test "verify: missing test is reported" {
    assert.is_type<root.engine.MissingTests>(verify_missing_test())
}

test "verify: expected rejection agrees" {
    assert.equal(verify_expected_rejection(), root.engine.Agrees {  })
}

test "verify: wrong code is unexpected diagnostics" {
    assert.is_type<root.engine.UnexpectedDiagnostics>(verify_wrong_code())
}

test "verify: wrong anchor is an anchor mismatch" {
    assert.is_type<root.engine.AnchorMismatch>(verify_wrong_anchor())
}

test "verify: anchor is found after multi-byte characters" {
    assert.equal(verify_anchor_after_multibyte(), root.engine.Agrees {  })
}

test "verify: rejected key on compiling case is unexpected compile" {
    assert.equal(verify_unexpected_compile(), root.engine.UnexpectedCompile {  })
}

// ─── items ───────────────────────────────────────────────────────────────────

function item_is_deterministic(item: root.engine.Item) -> bool throws never {
    // BUG: calling an interface method through an existential (`item.id()`,
    // `item.generate(..)`) from inside a closure that is itself nested in
    // another closure panics the emitter:
    //   crates/baml_compiler2_emit/src/emit.rs:2025 "undefined function: user.Item.id"
    // Minimal repro: `items().filter_map((item) -> { [0].every((i) -> { item.id() == "x" }); null })`.
    // One closure level is fine, so this helper keeps the inner loop closure-free.
    let id = item.id();
    let i = 0;
    while (i < 3) {
        let seed = root.engine.seed_for(id, i);
        if (item.generate(seed) != item.generate(seed)) {
            return false;
        }
        i = i + 1;
    }
    true
}

function nondeterministic_items() -> string[] throws never {
    root.bank.items().filter_map((item) -> {
        if (item_is_deterministic(item)) {
            null
        } else {
            item.id()
        }
    })
}

function nonconforming_items() -> string[] throws never {
    root.bank.items().filter_map((item) -> {
        let report = root.engine.verify_item(item, 4);
        if (root.engine.conforms(item.status(), report)) {
            null
        } else {
            `${item.id()}: ${report}`
        }
    })
}

function duplicate_item_ids() -> string[] throws never {
    let seen: map<string, bool> = {};
    root.bank.items().filter_map((item) -> {
        if (seen.has(item.id())) {
            item.id()
        } else {
            let _ = seen.set(item.id(), true);
            null
        }
    })
}

function duplicate_rule_names() -> string[] throws never {
    let seen: map<string, bool> = {};
    root.bank.rules().filter_map((rule) -> {
        if (seen.has(rule.name)) {
            rule.name
        } else {
            let _ = seen.set(rule.name, true);
            null
        }
    })
}

test "items: generation is deterministic" {
    assert.equal(nondeterministic_items().join("\n"), "")
}

test "items: every item conforms to its declared status" {
    assert.equal(nonconforming_items().join("\n"), "")
}

test "items: ids are unique" {
    assert.equal(duplicate_item_ids().join("\n"), "")
}

test "rules: names are unique" {
    assert.equal(duplicate_rule_names().join("\n"), "")
}
