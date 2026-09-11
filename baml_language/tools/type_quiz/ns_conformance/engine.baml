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

/// The region `needle` first occupies in `source`, in bytes: the character
/// index BAML's string methods use is converted through the prefix's UTF-8.
function region_of(
    file: string,
    source: string,
    needle: string,
) -> root.engine.Region throws never {
    let index = source.index_of(needle) ?? source.length();
    let start = source.slice(0, index).to_utf8().length();
    root.engine.Region { file, start, end: start + needle.to_utf8().length() }
}

function compiling_case(test_body: string) -> root.engine.Case throws never {
    root.engine.Case {
        files: root.engine.one_file(
            "case.baml",
            `
                function double(n: int) -> int { n * 2 }
                test "probe" { ${test_body} }
            `,
        ),
        key: root.engine.Compiles { tests: ["root::probe"] },
        trace: [],
    }
}

function mismatch_source() -> string throws never {
    `
        function mismatch() -> int {
            let text: int = "s";
            text
        }
    `
}

function rejected_case(code: string, at: root.engine.Region) -> root.engine.Case throws never {
    root.engine.Case {
        files: root.engine.one_file("case.baml", mismatch_source()),
        key: root.engine.Rejected { code, more: [], at },
        trace: [],
    }
}

function verify_passing() -> root.engine.Outcome throws never {
    root.engine.verify(compiling_case("assert.equal(double(2), 4)"))
}

function verify_failing_test() -> root.engine.Outcome throws never {
    root.engine.verify(compiling_case("assert.equal(double(2), 5)"))
}

/// A failing test the key does not list still fails the case.
function verify_unlisted_failing_test() -> root.engine.Outcome throws never {
    root.engine.verify(
        root.engine.Case {
            files: root.engine.one_file(
                "case.baml",
                `
                    test "listed" { assert.is_true(true) }
                    test "unlisted" { assert.is_true(false) }
                `,
            ),
            key: root.engine.Compiles { tests: ["root::listed"] },
            trace: [],
        },
    )
}

function verify_missing_test() -> root.engine.Outcome throws never {
    let case = compiling_case("assert.is_true(true)");
    root.engine.verify(
        root.engine.Case { ...case, key: root.engine.Compiles { tests: ["root::absent"] } },
    )
}

function verify_expected_rejection() -> root.engine.Outcome throws never {
    root.engine.verify(rejected_case("E0001", region_of("case.baml", mismatch_source(), "\"s\"")))
}

function verify_wrong_code() -> root.engine.Outcome throws never {
    root.engine.verify(rejected_case("E9999", region_of("case.baml", mismatch_source(), "\"s\"")))
}

/// A region the compiler does not point inside: the function's name.
function verify_wrong_region() -> root.engine.Outcome throws never {
    root.engine.verify(
        rejected_case("E0001", region_of("case.baml", mismatch_source(), "mismatch")),
    )
}

/// A key that says the case compiles when the compiler rejects it.
function verify_unexpected_diagnostics() -> root.engine.Outcome throws never {
    root.engine.verify(
        root.engine.Case {
            files: root.engine.one_file("case.baml", mismatch_source()),
            key: root.engine.Compiles { tests: [] },
            trace: [],
        },
    )
}

/// Multi-byte characters before the offending expression: byte offsets and
/// character indices diverge, and the region must be in bytes.
function multibyte_source() -> string throws never {
    `
        // 🐑🐑🐑
        function mismatch() -> int {
            let text: int = "s";
            text
        }
    `
}

function verify_region_after_multibyte() -> root.engine.Outcome throws never {
    root.engine.verify(
        root.engine.Case {
            files: root.engine.one_file("case.baml", multibyte_source()),
            key: root.engine.Rejected {
                code: "E0001",
                more: [],
                at: region_of("case.baml", multibyte_source(), "\"s\""),
            },
            trace: [],
        },
    )
}

/// The multibyte case is only a test of bytes if the character index and
/// the byte offset differ.
function multibyte_offsets_differ() -> bool throws never {
    let source = multibyte_source();
    let index = source.index_of("\"s\"") ?? -1;
    region_of("case.baml", source, "\"s\"").start != index
}

function verify_unexpected_compile() -> root.engine.Outcome throws never {
    let case = compiling_case("assert.is_true(true)");
    root.engine.verify(
        root.engine.Case {
            ...case,
            key: root.engine.Rejected {
                code: "E0001",
                more: [],
                at: root.engine.Region { file: "case.baml", start: 0, end: 1 },
            },
        },
    )
}

function failed_names(outcome: root.engine.Outcome) -> string[] throws never {
    match (outcome) {
        let f: root.engine.FailedTests => {
            f.failures.map((failure) -> {
                failure.name
            })
        },
        root.engine.Agrees
            | root.engine.UnexpectedDiagnostics
            | root.engine.UnexpectedCompile
            | root.engine.OtherCodes
            | root.engine.Mislocated
            | root.engine.InitThrew
            | root.engine.MissingTests =>
        {
            []
        },
    }
}

function failures_have_reasons(outcome: root.engine.Outcome) -> bool throws never {
    match (outcome) {
        let f: root.engine.FailedTests => {
            f.failures.every((failure) -> {
                failure.reason.length() > 0
            })
        },
        root.engine.Agrees
            | root.engine.UnexpectedDiagnostics
            | root.engine.UnexpectedCompile
            | root.engine.OtherCodes
            | root.engine.Mislocated
            | root.engine.InitThrew
            | root.engine.MissingTests =>
        {
            false
        },
    }
}

function reports_other_codes(outcome: root.engine.Outcome) -> bool throws never {
    match (outcome) {
        let o: root.engine.OtherCodes => o.expected == ["E9999"] && o.codes == ["E0001"],
        root.engine.Agrees
            | root.engine.UnexpectedDiagnostics
            | root.engine.UnexpectedCompile
            | root.engine.Mislocated
            | root.engine.InitThrew
            | root.engine.MissingTests
            | root.engine.FailedTests =>
        {
            false
        },
    }
}

function reports_mislocation(outcome: root.engine.Outcome) -> bool throws never {
    match (outcome) {
        let m: root.engine.Mislocated => {
            m.at == region_of("case.baml", mismatch_source(), "mismatch")
                && m.diagnostics.map((d) -> {
                    d.code
                })
                    == ["E0001"]
                && m.diagnostics.every((d) -> {
                    d.at != null
                })
        },
        root.engine.Agrees
            | root.engine.UnexpectedDiagnostics
            | root.engine.UnexpectedCompile
            | root.engine.OtherCodes
            | root.engine.InitThrew
            | root.engine.MissingTests
            | root.engine.FailedTests =>
        {
            false
        },
    }
}

function reports_unexpected_diagnostics(outcome: root.engine.Outcome) -> bool throws never {
    match (outcome) {
        let u: root.engine.UnexpectedDiagnostics => u.codes == ["E0001"] && u.messages.length() == 1,
        root.engine.Agrees
            | root.engine.UnexpectedCompile
            | root.engine.OtherCodes
            | root.engine.Mislocated
            | root.engine.InitThrew
            | root.engine.MissingTests
            | root.engine.FailedTests =>
        {
            false
        },
    }
}

test "verify: compiling case with passing test agrees" {
    assert.equal(verify_passing(), root.engine.Agrees {  })
}

test "verify: failing test is reported by name with a reason" {
    assert.equal(failed_names(verify_failing_test()), ["root::probe"]);
    assert.is_true(failures_have_reasons(verify_failing_test()))
}

test "verify: an unlisted failing test still fails the case" {
    assert.equal(failed_names(verify_unlisted_failing_test()), ["root::unlisted"])
}

test "verify: missing test is reported with what is defined" {
    assert.equal(
        verify_missing_test(),
        root.engine.MissingTests { names: ["root::absent"], defined: ["root::probe"] },
    )
}

test "verify: expected rejection agrees" {
    assert.equal(verify_expected_rejection(), root.engine.Agrees {  })
}

test "verify: wrong code reports both code lists" {
    assert.is_true(reports_other_codes(verify_wrong_code()))
}

test "verify: a region the compiler does not point inside is a mislocation" {
    assert.is_true(reports_mislocation(verify_wrong_region()))
}

test "verify: a compiling key on a rejected case reports the diagnostics" {
    assert.is_true(reports_unexpected_diagnostics(verify_unexpected_diagnostics()))
}

test "verify: regions are bytes, not characters" {
    assert.is_true(multibyte_offsets_differ());
    assert.equal(verify_region_after_multibyte(), root.engine.Agrees {  })
}

test "verify: rejected key on compiling case is unexpected compile" {
    assert.equal(verify_unexpected_compile(), root.engine.UnexpectedCompile {  })
}

// ─── conformance of a report to a status ─────────────────────────────────────

function report_of(outcomes: root.engine.Outcome[]) -> root.engine.ItemReport throws never {
    let checked: root.engine.Checked[] = baml.iter.Range.new(0, outcomes.length())
        .map((i) -> {
            root.engine.Checked { index: i, outcome: outcomes[i] }
        })
        .collect();
    root.engine.ItemReport {
        id: "synthetic",
        produced: outcomes.length(),
        disagreements: checked.filter((c) -> {
            !root.engine.agrees(c.outcome)
        }),
    }
}

function accepts() -> root.engine.Status throws never {
    root.engine.CompilerDiverges {
        divergence: "CD-000",
        contradiction: root.engine.Contradiction.Accepts,
    }
}

function nothing_conforms_when_nothing_was_produced() -> bool throws never {
    let empty = report_of([]);
    !root.engine.conforms(root.engine.Verified {  }, empty)
        && !root.engine.conforms(accepts(), empty)
        && !root.engine.conforms(root.engine.SpecOnly { gap: "G-000" }, empty)
}

function verified_needs_every_case_to_agree() -> bool throws never {
    root.engine.conforms(
        root.engine.Verified {  },
        report_of([root.engine.Agrees {  }, root.engine.Agrees {  }]),
    )
        && !root.engine.conforms(
            root.engine.Verified {  },
            report_of([root.engine.Agrees {  }, root.engine.UnexpectedCompile {  }]),
        )
}

function divergence_needs_every_case_to_contradict_as_declared() -> bool throws never {
    let other = root.engine.OtherCodes { expected: ["E0068"], codes: ["E0001"], messages: [] };
    root.engine.conforms(
        accepts(),
        report_of([root.engine.UnexpectedCompile {  }, root.engine.UnexpectedCompile {  }]),
    )
        && !root.engine.conforms(
            accepts(),
            report_of([root.engine.UnexpectedCompile {  }, root.engine.Agrees {  }]),
        )
        && !root.engine.conforms(accepts(), report_of([other, other]))
}

function spec_only_needs_every_case_to_disagree() -> bool throws never {
    let gap = root.engine.SpecOnly { gap: "G-000" };
    root.engine.conforms(gap, report_of([root.engine.UnexpectedCompile {  }]))
        && !root.engine.conforms(
            gap,
            report_of([root.engine.UnexpectedCompile {  }, root.engine.Agrees {  }]),
        )
}

test "conforms: a status promises nothing of an item that produced nothing" {
    assert.is_true(nothing_conforms_when_nothing_was_produced())
}

test "conforms: verified means every produced case agrees" {
    assert.is_true(verified_needs_every_case_to_agree())
}

test "conforms: a divergence means every produced case contradicts as declared" {
    assert.is_true(divergence_needs_every_case_to_contradict_as_declared())
}

test "conforms: spec-only means every produced case disagrees" {
    assert.is_true(spec_only_needs_every_case_to_disagree())
}

// ─── items ───────────────────────────────────────────────────────────────────

function item_is_deterministic(item: root.engine.Item) -> bool throws never {
    let id = item.id();
    baml.iter.Range.new(0, 3).every((i) -> {
        let seed = root.engine.seed_for(id, i);
        root.engine.generate(item, seed) == root.engine.generate(item, seed)
    })
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

/// Two seeds of every item, each of which puts both of the item's programs
/// to the compiler where it has two.
///
/// Two rather than four because a seed now costs two compilations instead of
/// one, and it buys more: the direction a case flows is a coin flip per
/// seed, so four seeds left an item's other direction unchecked about one
/// time in eight, while a foil is the other direction by construction and is
/// checked every time.
function nonconforming_items() -> string[] throws never {
    root.bank.items().filter_map((item) -> {
        let report = root.engine.verify_item(item, 2);
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

// ─── how a case is put ───────────────────────────────────────────────────────
//
// A candidate with a foil can be put two ways, and which way is a draw
// against a knob. What must hold either way: the pair really contrasts, the
// order of the two carries nothing, and an answer is judged against the
// question that was asked rather than the program that was shown.

function claim_of(name: string) -> root.engine.Claim throws never {
    root.engine.Claim {
        rule: root.engine.Rule {
            name,
            spec: root.engine.Section { heading: "Taxonomy" },
            quote: "a quote",
            more: [],
        },
        instance: `an instance of ${name}`,
    }
}

/// A case with a trace, so a pair can be told to part where it should.
function traced(compiles: bool, source: string, names: string[]) -> root.engine.Case throws never {
    root.engine.Case {
        files: root.engine.one_file("case.baml", source),
        key: if (compiles) {
            root.engine.Compiles { tests: [] }
        } else {
            root.engine.Rejected {
                code: "E0001",
                more: [],
                at: root.engine.Region { file: "case.baml", start: 0, end: 1 },
            }
        },
        trace: names.map((name) -> {
            claim_of(name)
        }),
    }
}

/// A candidate whose item has a second program to offer.
function paired(compiles: bool) -> root.engine.Candidate throws never {
    root.engine.Candidate {
        case: traced(compiles, "one", ["axiom", "one-way"]),
        features: {},
        traps: ["spelling"],
    }
}

/// The foil that answers the other way.
function opposing(compiles: bool) -> root.engine.Foil throws never {
    root.engine.Foil {
        case: traced(!compiles, "two", ["axiom", "other-way"]),
        difference: "the two types swap places",
        traps: ["typescript"],
    }
}

/// A foil that answers the same way: not a pair at all.
function agreeing() -> root.engine.Foil throws never {
    root.engine.Foil {
        case: traced(true, "two", ["axiom", "other-way"]),
        difference: "nothing that matters",
        traps: [],
    }
}

function put_with(
    foil: root.engine.Foil?,
    knobs: root.engine.Knobs,
    seed: root.engine.Seed,
) -> root.engine.Put throws never {
    root.engine.put(
        paired(true),
        () -> {
            foil
        },
        knobs,
        seed,
    )
}

/// How often the foil is asked for over a sweep at a rate of a third, and
/// how many choices came of it. Building one means rendering a second whole
/// program, and most candidates are drawn only to be thrown away, so the two
/// numbers must agree.
function builds_only_when_needed() -> string throws never {
    let asked: map<string, int> = { "count": 0 };
    let choices = baml.iter.Range.new(0, 200)
        .filter((i) -> {
            let put = root.engine.put(
                paired(true),
                () -> {
                    let _ = asked.set("count", (asked.get("count") ?? 0) + 1);
                    opposing(true)
                },
                knobs_at(0.35),
                root.engine.seed_for("share", i),
            );
            is_choice(put)
        })
        .count();
    `${asked.get("count") ?? 0} for ${choices}`
}

function knobs_at(contrast: float) -> root.engine.Knobs throws never {
    root.engine.Knobs { ...root.engine.default_knobs(), contrast }
}

function is_choice(put: root.engine.Put) -> bool throws never {
    put.posed is root.engine.Choice
}

/// Over a sweep of seeds: how often each kind of candidate is put as a
/// choice, and whether putting one twice puts it the same way.
function putting_report() -> string throws never {
    let seeds = baml.iter.Range.new(0, 200)
        .map((i) -> {
            root.engine.seed_for("putting", i)
        })
        .collect();
    let never_ = seeds
        .filter((seed) -> {
            is_choice(put_with(opposing(true), knobs_at(0.0), seed))
        })
        .length();
    let always = seeds
        .filter((seed) -> {
            is_choice(put_with(opposing(true), knobs_at(1.0), seed))
        })
        .length();
    let unpaired = seeds
        .filter((seed) -> {
            is_choice(put_with(null, knobs_at(1.0), seed))
        })
        .length();
    let same_way = seeds
        .filter((seed) -> {
            is_choice(put_with(agreeing(), knobs_at(1.0), seed))
        })
        .length();
    let stable = seeds.every((seed) -> {
        put_with(opposing(true), root.engine.default_knobs(), seed)
            == put_with(opposing(true), root.engine.default_knobs(), seed)
    });
    // A foil is built only when the draw calls for one, so a candidate put
    // as a verdict never asks its item for a second program.
    `never=${never_} always=${always} unpaired=${unpaired} agreeing_foil=${same_way} stable=${stable} asked=${builds_only_when_needed()}`
}

/// The knob is the share: at 0.35, 68 of 200 draws are choices.
function contrast_share() -> string throws never {
    let choices = baml.iter.Range.new(0, 200)
        .filter((i) -> {
            is_choice(put_with(opposing(true), knobs_at(0.35), root.engine.seed_for("share", i)))
        })
        .count();
    `choices=${choices}`
}

/// Which of the two is shown first is the only thing `compiles_first` says,
/// and both halves of the question follow from it.
function order_report() -> string throws never {
    let first = root.engine.contrast(paired(true).case, opposing(true), true)
        ?? return "no choice";
    let second = root.engine.contrast(paired(true).case, opposing(true), false)
        ?? return "no choice";
    let sources = [first, second].map((choice) -> {
        root.engine.cases_of(choice).map((case) -> {
            case.files.get("case.baml") ?? "?"
        })
    });
    `shown=${sources} second_compiles=${[first.second_compiles(), second.second_compiles()]}`
}

/// A question of two programs turns on what they agree about, and its reveal
/// keeps that apart from what tells them apart.
function shared_report() -> string throws never {
    let choice = root.engine.contrast(paired(true).case, opposing(true), true)
        ?? return "no choice";
    let revealed = root.engine.reveal(choice);
    let alone = root.engine.reveal(paired(true).case);
    `rules=${root.engine.rules_of(choice)} tails=${revealed.programs.map((p) -> {
        p.claims.length()
    })} difference=${revealed.difference != ""} alone_rules=${root.engine.rules_of(paired(true).case)} alone_tail=${alone.programs[0].claims.length()}`
}

test "put: the knob decides, a candidate without a pair is never a choice, and putting is stable" {
    assert.equal(
        putting_report(),
        "never=0 always=200 unpaired=0 agreeing_foil=0 stable=true asked=68 for 68",
    )
}

test "put: the knob is the share of the draws it takes" {
    assert.equal(contrast_share(), "choices=68")
}

test "choice: the order of the two is all that `compiles_first` decides" {
    assert.equal(
        order_report(),
        `shown=[["one", "two"], ["two", "one"]] second_compiles=[false, true]`,
    )
}

test "choice: a question turns on what its programs agree about" {
    assert.equal(
        shared_report(),
        `rules=["axiom"] tails=[1, 1] difference=true alone_rules=["axiom", "one-way"] alone_tail=0`,
    )
}

// ─── judging what was asked ──────────────────────────────────────────────────

function choice_question() -> root.engine.Question throws never {
    let choice = root.engine.contrast(paired(true).case, opposing(true), true)
        ?? return question_of_case();
    root.engine.Question { item: "synthetic", seed: 1, posed: choice }
}

function question_of_case() -> root.engine.Question throws never {
    root.engine.Question { item: "synthetic", seed: 1, posed: paired(true).case }
}

function chose(second: bool) -> root.engine.Answer throws never {
    root.engine.Answer { said: root.engine.Chose { second }, reasoning: "" }
}

/// A judgement in a word, since it has three outcomes and not two.
function judged_word(verdict_correct: bool?) -> string throws never {
    match (verdict_correct) {
        let right: bool => {
            if (right) {
                "right"
            } else {
                "wrong"
            }
        },
        null => "withheld",
    }
}

/// The first of the two compiles, so saying the first is right and saying
/// the second is wrong; withholding is neither.
function judging_a_choice() -> string throws never {
    let question = choice_question();
    let no_answer = root.engine.Answer { said: null, reasoning: "" };
    let marks = [chose(false), chose(true), no_answer].map((answer) -> {
        judged_word(root.engine.judge(question, answer, null).verdict_correct)
    });
    `marks=${marks}`
}

/// An answer of the wrong kind for the question is a defect in whatever
/// asked it, and says so rather than being read as an answer.
function crossed_answers() -> string throws never {
    let verdict_on_choice = {
        let judgement = root.engine.judge(
            choice_question(),
            root.engine.Answer { said: root.engine.VerdictKind.Compiles, reasoning: "" },
            null,
        );
        judged_word(judgement.verdict_correct)
    } catch (e) {
        baml.panics.Panic => "panicked",
    };
    let choice_on_verdict = {
        let judgement = root.engine.judge(question_of_case(), chose(true), null);
        judged_word(judgement.verdict_correct)
    } catch (e) {
        baml.panics.Panic => "panicked",
    };
    `verdict_on_choice=${verdict_on_choice} choice_on_verdict=${choice_on_verdict}`
}

test "judge: a choice is right when it names the program the compiler accepts" {
    assert.equal(judging_a_choice(), `marks=["right", "wrong", "withheld"]`)
}

test "judge: an answer of the wrong kind for the question is refused" {
    assert.equal(crossed_answers(), "verdict_on_choice=panicked choice_on_verdict=panicked")
}

