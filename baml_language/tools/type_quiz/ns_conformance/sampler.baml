// Conformance suite for the sampler and the naive models: scoring and hinges
// behave; every model override names a rule, is keyed by a rule of the kind
// of step it applies to, and produces a trap somewhere in the bank; and what
// a session serves is verified, balanced, and predictable from nothing but
// the answer.

// ─── scoring ─────────────────────────────────────────────────────────────────

function weights_for(name: string, w: root.engine.Weight) -> root.engine.Weights throws never {
    // `{ name: w }` would key the map by the literal string "name".
    let entries: map<string, root.engine.Weight> = {};
    entries.set(name, w);
    root.engine.Weights { entries }
}

function feature(name: string, value: float) -> map<string, float> throws never {
    let m: map<string, float> = {};
    m.set(name, value);
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

function value_at_cap_is_kept() -> bool throws never {
    let w = weights_for(
        "x",
        root.engine.Weight {
            weight: -1.0,
            hinge: root.engine.Hinge { allowance: 0.0, hard_cap: 1.0 },
        },
    );
    root.engine.score(feature("x", 1.0), w) == -1.0
}

function entries_add_up() -> bool throws never {
    let w = root.engine.Weights {
        entries: {
            "x": root.engine.Weight { weight: 2.0, hinge: null },
            "y": root.engine.Weight { weight: 3.0, hinge: null },
        },
    };
    let features: map<string, float> = { "x": 1.0, "y": 1.0 };
    root.engine.score(features, w) == 5.0
}

test "score: a hard cap rejects the candidate" {
    assert.is_true(hard_cap_rejects())
}

test "score: a value at the hard cap is kept" {
    assert.is_true(value_at_cap_is_kept())
}

test "score: entries add up" {
    assert.is_true(entries_add_up())
}

// ─── the sampler's primitives ────────────────────────────────────────────────

function unit_stays_in_range() -> bool throws never {
    root.engine.streams(root.engine.seed_from(11), 500).every((s) -> {
        let u = root.engine.unit(s);
        u >= 0.0 && u < 1.0
    })
}

function streams_are_distinct() -> bool throws never {
    let states = root.engine.streams(root.engine.seed_from(12), 50).map((s) -> {
        s.state
    });
    distinct(
        states.map((state) -> {
            state.to_string()
        }),
    )
        .length()
        == states.length()
}

/// A candidate with nothing but a score, for the picker.
function scored(score: float) -> root.engine.Scored throws never {
    root.engine.Scored {
        item: "synthetic",
        seed: root.engine.seed_from(0),
        candidate: root.engine.Candidate {
            case: root.engine.Case {
                files: root.engine.one_file("case.baml", ""),
                key: root.engine.Compiles { tests: [] },
                trace: [],
            },
            features: {},
            traps: [],
        },
        score,
    }
}

function pick_of_nothing_is_null() -> bool throws never {
    root.engine.pick([], root.engine.seed_from(1)) == null
}

/// With scores `0` and `ln 3` the second candidate has weight 3 against 1,
/// so it should be picked about three times in four.
function picks_of_the_heavier() -> Share throws never {
    let pair = [scored(0.0), scored(baml.Float.ln(3.0))];
    let seeds = root.engine.streams(root.engine.seed_from(5), 400);
    Share {
        hits: seeds
            .filter((s) -> {
                (root.engine.pick(pair, s)?.score ?? -1.0) > 0.5
            })
            .length(),
        total: seeds.length(),
    }
}

function draw_respects_count_and_caps() -> bool throws never {
    let item = root.bank.items()[0];
    let seed = root.engine.seed_from(8);
    let capped = weights_for(
        "size",
        root.engine.Weight {
            weight: 0.0,
            hinge: root.engine.Hinge { allowance: 0.0, hard_cap: 0.0 },
        },
    );
    let some = root.engine.draw(item, seed, root.bank.default_weights(), 5);
    some.length() > 0
        && some.length() <= 5
        && root.engine.draw(item, seed, capped, 5).length() == 0
        && root.engine.draw(item, seed, root.bank.default_weights(), 0).length() == 0
}

function schedule_is_balanced_and_seeded() -> bool throws never {
    let compiling = (kinds: root.engine.VerdictKind[]) -> {
        kinds
            .filter((k) -> {
                k == root.engine.VerdictKind.Compiles
            })
            .length()
    };
    let even = root.engine.schedule(root.engine.seed_from(9), 120).value;
    let odd = root.engine.schedule(root.engine.seed_from(9), 7).value;
    let other = root.engine.schedule(root.engine.seed_from(10), 120).value;
    even.length() == 120
        && compiling(even) == 60
        && odd.length() == 7
        && compiling(odd) == 4
        && compiling(other) == 60
        && even != other
        && root.engine.schedule(root.engine.seed_from(0), 0).value.length() == 0
}

test "unit: draws stay in [0, 1)" {
    assert.is_true(unit_stays_in_range())
}

test "streams: child streams are distinct" {
    assert.is_true(streams_are_distinct())
}

test "pick: nothing from nothing" {
    assert.is_true(pick_of_nothing_is_null())
}

test "pick: proportional to exp(score)" {
    assert.equal(check("heavier candidate picked", picks_of_the_heavier(), 0.65, 0.85), null)
}

test "draw: respects the count and drops capped candidates" {
    assert.is_true(draw_respects_count_and_caps())
}

test "schedule: balanced, seeded, and empty for zero length" {
    assert.is_true(schedule_is_balanced_and_seeded())
}

// ─── a seed sweep over the bank ──────────────────────────────────────────────

/// Every candidate the bank produces for the first `per_item` seeds of each
/// item.
function sweep(per_item: int) -> root.engine.Candidate[] throws never {
    root.bank.items().flat_map((item) -> {
        let id = item.id();
        baml.iter.Range.new(0, per_item)
            .filter_map((i) -> {
                root.engine.generate(item, root.engine.seed_for(id, i))
            })
            .collect()
    })
}

function distinct(names: string[]) -> string[] throws never {
    let none: string[] = [];
    names.reduce(
        (acc, name) -> {
            if (acc.includes(name)) {
                acc
            } else {
                acc.concat([name])
            }
        },
        none,
    )
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

/// The rules the bank's facts use as asserted steps and as transferred
/// steps.
class StepKinds {
    asserted: string[],
    transferred: string[],
}

function step_kinds(per_fact: int) -> StepKinds throws never {
    let steps = root.bank.facts().flat_map((fact) -> {
        let generate = fact.generate;
        baml.iter.Range.new(0, per_fact)
            .flat_map((i) -> {
                root.algebra.steps(generate(root.engine.seed_for(fact.name, i)).value)
            })
            .collect()
    });
    StepKinds {
        asserted: distinct(
            steps.filter_map((step) -> {
                match (step) {
                    let a: root.algebra.Asserted => a.rule.name,
                    root.algebra.Transferred => null,
                }
            }),
        ),
        transferred: distinct(
            steps.filter_map((step) -> {
                match (step) {
                    root.algebra.Asserted => null,
                    let t: root.algebra.Transferred => t.rule.name,
                }
            }),
        ),
    }
}

/// A belief override applies only to an asserted step and a transfer
/// override only to a transferred one, so an override keyed by a rule of the
/// other kind would never fire.
function overrides_of_wrong_kind() -> string[] throws never {
    let kinds = step_kinds(4);
    root.bank.models().flat_map((model) -> {
        model.believes.keys()
            .filter_map((key) -> {
                if (kinds.asserted.includes(key)) {
                    null
                } else {
                    `${model.id}: believes ${key}, which no fact asserts`
                }
            })
            .concat(
                model.transfers.keys().filter_map((key) -> {
                    if (kinds.transferred.includes(key)) {
                        null
                    } else {
                        `${model.id}: transfers ${key}, which no fact transfers through`
                    }
                }),
            )
    })
}

/// A rule used both ways would make an override on it half-apply.
function rules_used_both_ways() -> string[] throws never {
    let kinds = step_kinds(4);
    kinds.asserted.filter((name) -> {
        kinds.transferred.includes(name)
    })
}

/// An override that never turns a case into a trap is dead.
function dead_overrides() -> string[] throws never {
    let candidates = sweep(8);
    root.bank.models().flat_map((model) -> {
        override_keys(model).filter_map((key) -> {
            let live = candidates.some((c) -> {
                c.traps.includes(model.id)
                    && c.case.trace.some((claim) -> {
                        claim.rule.name == key
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

/// How many of `cases` compile.
class Share {
    hits: int,
    total: int,
}

function compiling_share(cases: root.engine.Candidate[]) -> Share throws never {
    Share {
        hits: cases
            .filter((c) -> {
                root.engine.verdict_kind(c.case.key) == root.engine.VerdictKind.Compiles
            })
            .length(),
        total: cases.length(),
    }
}

/// Models trapped by compiling and by rejected cases. A model that accepts
/// strictly less or strictly more than the spec is trapped in one direction
/// only; the bank must hold at least one that is not, or a learner who has
/// recognised a trap has also learnt its answer.
function two_sided_models() -> string[] throws never {
    let candidates = sweep(8);
    root.bank.models().filter_map((model) -> {
        let share = compiling_share(
            candidates.filter((c) -> {
                c.traps.includes(model.id)
            }),
        );
        if (share.hits > 0 && share.hits < share.total) {
            model.id
        } else {
            null
        }
    })
}

test "models: every override names a rule" {
    assert.equal(overrides_naming_no_rule().join("\n"), "")
}

test "models: every override is keyed by a rule of the kind it applies to" {
    assert.equal(overrides_of_wrong_kind().join("\n"), "")
}

test "models: no rule is both asserted and transferred through" {
    assert.equal(rules_used_both_ways().join("\n"), "")
}

test "models: every override produces a trap somewhere in the bank" {
    assert.equal(dead_overrides().join("\n"), "")
}

test "models: some model is trapped in both directions" {
    assert.is_true(two_sided_models().length() > 0)
}

// ─── what a session serves ───────────────────────────────────────────────────

/// A 120-turn session at a fixed seed with the default weights.
function selected() -> root.engine.Turn[] throws never {
    root.engine.session(
        root.bank.items(),
        root.engine.seed_from(2024),
        root.bank.default_weights(),
        120,
        root.bank.default_budget(),
    )
}

function served_candidates() -> root.engine.Candidate[] throws never {
    root.engine.served(selected()).map((turn) -> {
        turn.chosen.candidate
    })
}

function within(share: Share, low: float, high: float) -> bool throws never {
    if (share.total == 0) {
        return false;
    }
    let p = share.hits * 1.0 / (share.total * 1.0);
    p >= low && p <= high
}

/// `null` when `share` sits in `[low, high]`, else a line saying how it
/// does not.
function check(label: string, share: Share, low: float, high: float) -> string? throws never {
    if (within(share, low, high)) {
        null
    } else {
        `${label}: ${share.hits}/${share.total}, outside [${low}, ${high}]`
    }
}

function size_of(c: root.engine.Candidate) -> float throws never {
    c.features.get("size") ?? 0.0
}

function balance_report() -> string[] throws never {
    let cases = served_candidates();
    let traps = cases.filter((c) -> {
        c.traps.length() > 0
    });
    let sizes = cases
        .map((c) -> {
            size_of(c)
        })
        .sort_by_key((s) -> {
            s
        });
    let median = sizes.at(sizes.length() / 2) ?? 0.0;
    let small = cases.filter((c) -> {
        size_of(c) < median
    });
    let large = cases.filter((c) -> {
        size_of(c) >= median
    });
    [
        check("compiling overall", compiling_share(cases), 0.4, 0.6),
        check("compiling among traps", compiling_share(traps), 0.3, 0.7),
        check("compiling among small cases", compiling_share(small), 0.3, 0.7),
        check("compiling among large cases", compiling_share(large), 0.3, 0.7),
    ]
        .filter_map((line) -> {
            line
        })
}

function served_kinds() -> root.engine.VerdictKind[] throws never {
    root.engine.served(selected()).map((turn) -> {
        root.engine.verdict_kind(turn.chosen.candidate.case.key)
    })
}

/// The verdict at a step must not be guessable from the step's index or
/// from the step before: a parity guess and a "same as last time" guess
/// must both land near one half.
function serial_report() -> string[] throws never {
    let kinds = served_kinds();
    let n = kinds.length();
    if (n < 2) {
        return ["fewer than two turns were served"];
    }
    let parity = baml.iter.Range.new(0, n)
        .filter((i) -> {
            (kinds[i] == root.engine.VerdictKind.Compiles) == (i % 2 == 0)
        })
        .count();
    let repeats = baml.iter.Range.new(1, n)
        .filter((i) -> {
            kinds[i] == kinds[i - 1]
        })
        .count();
    [
        check("parity guesses right", Share { hits: parity, total: n }, 0.35, 0.65),
        check("same verdict as the step before", Share { hits: repeats, total: n - 1 }, 0.35, 0.65),
    ]
        .filter_map((line) -> {
            line
        })
}

/// Every turn of the fixed session has a case, and the schedule is honoured
/// outright: the budget's re-draws must leave no turn to fall back.
function session_health() -> string[] throws never {
    let turns = selected();
    let served = root.engine.served(turns);
    let fallbacks = served
        .filter((turn) -> {
            root.engine.fell_back(turn)
        })
        .length();
    [
        check("turns served", Share { hits: served.length(), total: turns.length() }, 1.0, 1.0),
        check("turns that fell back", Share { hits: fallbacks, total: served.length() }, 0.0, 0.0),
    ]
        .filter_map((line) -> {
            line
        })
}

function unverified_items_served() -> string[] throws never {
    let verified = root.bank.items().filter_map((item) -> {
        if (item.status() is root.engine.Verified) {
            item.id()
        } else {
            null
        }
    });
    distinct(
        root.engine.served(selected()).filter_map((turn) -> {
            if (verified.includes(turn.chosen.item)) {
                null
            } else {
                turn.chosen.item
            }
        }),
    )
}

/// Every served case, through the compiler.
function served_disagreements() -> string[] throws never {
    root.engine.served(selected()).filter_map((turn) -> {
        let outcome = root.engine.verify(turn.chosen.candidate.case);
        if (root.engine.agrees(outcome)) {
            null
        } else {
            `${turn.chosen.item}: ${outcome}`
        }
    })
}

function session_is_deterministic() -> bool throws never {
    selected() == selected()
}

function boring_share() -> float throws never {
    let cases = served_candidates();
    if (cases.length() == 0) {
        return 1.0;
    }
    let boring = cases
        .filter((c) -> {
            (c.features.get("boring") ?? 0.0) > 0.5
        })
        .length();
    boring * 1.0 / (cases.length() * 1.0)
}

test "session: the verdict is balanced overall, among traps, and across size" {
    assert.equal(balance_report().join("\n"), "")
}

test "session: the verdict is not predictable from the step index or the step before" {
    assert.equal(serial_report().join("\n"), "")
}

test "session: every turn is served and fallbacks are rare" {
    assert.equal(session_health().join("\n"), "")
}

test "session: only verified items are served" {
    assert.equal(unverified_items_served().join("\n"), "")
}

test "session: every served case agrees with the compiler" {
    assert.equal(served_disagreements().join("\n"), "")
}

test "session: the same seed serves the same turns" {
    assert.is_true(session_is_deterministic())
}

test "session: single-rule cases every model agrees with are rare" {
    assert.is_true(boring_share() < 0.25)
}
