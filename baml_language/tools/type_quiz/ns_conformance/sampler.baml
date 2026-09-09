// Conformance suite for the sampler and the naive models: scoring and hinges
// behave, every model override names a rule and produces a trap somewhere,
// override kinds match the steps they apply to, and the verdict of what a
// session selects is independent of interest and of size.
//
// Test-block locals are boxed by the VM, so every case body is a function and
// the `test` blocks only assert.

// ─── scoring ─────────────────────────────────────────────────────────────────

function weights_for(name: string, w: root.engine.Weight) -> root.engine.Weights throws never {
    // `{ name: w }` would key the map by the literal string "name".
    let entries: map<string, root.engine.Weight> = {};
    let _ = entries.set(name, w);
    root.engine.Weights { entries: entries }
}

function feature(name: string, value: float) -> map<string, float> throws never {
    let m: map<string, float> = {};
    let _ = m.set(name, value);
    m
}

function plain_weight_is_linear() -> bool throws never {
    let w = weights_for("x", root.engine.Weight { weight: 2.0, hinge: null });
    root.engine.score(feature("x", 3.0), w) == 6.0 && root.engine.score(feature("y", 3.0), w) == 0.0
}

function hinge_is_free_then_quadratic() -> bool throws never {
    let w = weights_for(
        "x",
        root.engine.Weight {
            weight: -1.0,
            hinge: root.engine.Hinge { allowance: 2.0, hard_cap: 10.0 },
        },
    );
    root.engine.score(feature("x", 1.0), w) == 0.0
        && root.engine.score(feature("x", 2.0), w) == 0.0
        && root.engine.score(feature("x", 5.0), w) == -9.0
}

function hard_cap_rejects() -> bool throws never {
    let w = weights_for(
        "x",
        root.engine.Weight {
            weight: 0.0,
            hinge: root.engine.Hinge { allowance: 0.0, hard_cap: 0.5 },
        },
    );
    root.engine.score(feature("x", 1.0), w) == null
        && root.engine.score(feature("x", 0.0), w) == 0.0
}

test "score: a plain weight is linear and absent features read as zero" {
    assert.is_true(plain_weight_is_linear())
}

test "score: a hinge is free up to its allowance and quadratic past it" {
    assert.is_true(hinge_is_free_then_quadratic())
}

test "score: a hard cap rejects the candidate" {
    assert.is_true(hard_cap_rejects())
}

// ─── a seed sweep over the bank ──────────────────────────────────────────────

/// Every candidate the bank produces for the first `per_item` seeds of each
/// item.
function sweep(per_item: int) -> root.engine.Candidate[] throws never {
    let out: root.engine.Candidate[] = [];
    for (let item in root.bank.items()) {
        let i = 0;
        while (i < per_item) {
            if let c: root.engine.Candidate = item.generate(root.engine.seed_for(item.id(), i)) {
                let _ = out.push(c);
            }
            i = i + 1;
        }
    }
    out
}

// ─── models ──────────────────────────────────────────────────────────────────

function override_keys(model: root.algebra.Model) -> string[] throws never {
    model.believes.keys().concat(model.transfers.keys())
}

function overrides_naming_no_rule() -> string[] throws never {
    let names = root.bank.rules().map((r) -> {
        r.name
    });
    root.bank.models().flat_map((model) -> {
        override_keys(model).filter_map((key) -> {
            if (names.includes(key)) {
                null
            } else {
                `${model.id}: ${key} is not a rule`
            }
        })
    })
}

/// An override that never turns a case into a trap is dead.
function dead_overrides() -> string[] throws never {
    let candidates = sweep(8);
    root.bank.models().flat_map((model) -> {
        override_keys(model).filter_map((key) -> {
            let live = candidates.some((c) -> {
                c.traps.includes(model.id)
                    && c.case.trace.some((r) -> {
                        r.name == key
                    })
            });
            if (live) {
                null
            } else {
                `${model.id}: ${key} never traps`
            }
        })
    })
}

test "models: every override names a rule" {
    assert.equal(overrides_naming_no_rule().join("\n"), "")
}

test "models: every override produces a trap somewhere in the bank" {
    assert.equal(dead_overrides().join("\n"), "")
}

// ─── verdict independence ────────────────────────────────────────────────────

class Ratio {
    compiling: int,
    total: int,
}

function ratio(cases: root.engine.Scored[]) -> Ratio throws never {
    Ratio {
        compiling: cases
            .filter((c) -> {
                root.engine.is_compiling(c.candidate)
            })
            .length(),
        total: cases.length(),
    }
}

function within(r: Ratio, low: float, high: float) -> bool throws never {
    if (r.total == 0) {
        return false;
    }
    let p = r.compiling * 1.0 / (r.total * 1.0);
    p >= low && p <= high
}

function describe(label: string, r: Ratio) -> string throws never {
    `${label}: ${r.compiling}/${r.total} compile`
}

function selected() -> root.engine.Scored[] throws never {
    root.engine.session(
        root.bank.items(),
        root.engine.seed_from(2024),
        root.bank.default_weights(),
        120,
        3,
        3,
    )
}

function size_of(c: root.engine.Scored) -> float throws never {
    c.candidate.features.get("size") ?? 0.0
}

function balance_report() -> string[] throws never {
    let cases = selected();
    let traps = cases.filter((c) -> {
        c.candidate.traps.length() > 0
    });
    let sizes = cases
        .map((c) -> {
            size_of(c)
        })
        .sort_by_key((s) -> {
            s
        });
    let median = sizes[sizes.length() / 2];
    let small = cases.filter((c) -> {
        size_of(c) < median
    });
    let large = cases.filter((c) -> {
        size_of(c) >= median
    });
    let checks = [
        [
            describe("overall", ratio(cases)),
            if (within(ratio(cases), 0.4, 0.6)) {
                ""
            } else {
                "out of band"
            },
        ],
        [
            describe("among traps", ratio(traps)),
            if (within(ratio(traps), 0.3, 0.7)) {
                ""
            } else {
                "out of band"
            },
        ],
        [
            describe("small", ratio(small)),
            if (within(ratio(small), 0.3, 0.7)) {
                ""
            } else {
                "out of band"
            },
        ],
        [
            describe("large", ratio(large)),
            if (within(ratio(large), 0.3, 0.7)) {
                ""
            } else {
                "out of band"
            },
        ],
    ];
    checks.filter_map((c) -> {
        if (c[1] == "") {
            null
        } else {
            `${c[0]} (${c[1]})`
        }
    })
}

test "session: the verdict is balanced overall, among traps, and across size" {
    assert.equal(balance_report().join("\n"), "")
}

function session_determinism() -> string throws never {
    let a = selected().map((c) -> {
        c.candidate.case.files["case.baml"]
    });
    let b = selected().map((c) -> {
        c.candidate.case.files["case.baml"]
    });
    `same=${a == b} length=${a.length()}`
}

test "session: the same seed selects the same cases, one per step" {
    assert.equal(session_determinism(), "same=true length=120")
}

function boring_share() -> float throws never {
    let cases = selected();
    let boring = cases
        .filter((c) -> {
            (c.candidate.features.get("boring") ?? 0.0) > 0.5
        })
        .length();
    boring * 1.0 / (cases.length() * 1.0)
}

test "session: single-rule cases every model agrees with are rare" {
    assert.is_true(boring_share() < 0.25)
}
