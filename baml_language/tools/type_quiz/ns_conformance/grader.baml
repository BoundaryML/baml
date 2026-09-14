// The grader's calibration fixtures: a sound piece of reasoning and a wrong
// one, marked by a real model. Live only, under the `live` profile, and only
// when a key is in the environment — never in CI, which runs offline.

/// The case every fixture is marked against, revealed as a sitting reveals
/// it: one program the compiler accepts, and the two claims that say why.
///
/// A whole `Revealed` rather than the pieces a grader is told, because that
/// is what the surface takes: the fixture exercises the same working-out the
/// page does, including which claims the rubric draws from the answer.
function fixture_revealed() -> root.engine.Revealed throws never {
    root.engine.Revealed {
        programs: [
            root.engine.Verdicted {
                files: root.engine.one_file(
                    "main.baml",
                    "function settle(value: 1) -> void {\n    let target: int = value;\n}\n",
                ),
                compiles: true,
                reported: root.engine.Reported { compiles: true, codes: [], messages: [] },
                claims: [],
            },
        ],
        shared: fixture_claims(),
        difference: "",
    }
}

/// The rubric the fixture's reasoning is marked against, on the bank's own
/// rules rather than invented ones.
function fixture_claims() -> root.engine.Claim[] throws never {
    [
        root.engine.Claim {
            rule: root.bank.literal_subtype_of_concrete(),
            instance: "`1` is a strict subtype of `int`",
            bearing: true,
        },
        root.engine.Claim {
            rule: root.bank.flow_requires_subtype(),
            instance: "at the initializer of an annotated binding, a value of type `1` meets a slot of type `int`: `1` <: `int` holds, so the program compiles",
            bearing: true,
        },
    ]
}

/// The key the fixtures run with, or empty when there is none, which is what
/// keeps them off every run that did not ask for them.
function live_key() -> string throws never {
    baml.env.get("ANTHROPIC_API_KEY") catch_all (e) {
        baml.errors.Io => "",
        baml.errors.ParseError => "",
    }
        ?? ""
}

/// A provider error is a failed fixture, not something to absorb: these run
/// only when someone has asked for them with a key, and then they should say
/// what went wrong.
function marked(reasoning: string) -> root.engine.Grade {
    let judged = root.engine.judge_reasoning(
        live_key(),
        "claude-sonnet-4-5",
        fixture_revealed(),
        "compiles",
        reasoning,
    );
    match (judged) {
        let grade: root.engine.Grade => grade,
        let unjudged: root.engine.Unjudged => {
            throw baml.panics.Unreachable { message: `the judge refused: ${unjudged.why}` }
        },
    }
}

testset "live" with testing.Sequential() {
    // Always registered, so a keyless `--profile live` still selects
    // something rather than exiting on an empty selection.
    test "preconditions" {
        assert.is_true(true)
    }
    if (live_key() != "") {
        test "sound reasoning is marked sound" {
            let grade = marked(
                "A literal type like `1` is a subtype of its concrete type `int`, and a `let` with an annotation accepts any subtype of the annotation, so this compiles.",
            );
            assert.is_true(grade.understanding == root.engine.Understanding.Sound)
        }
        test "a right answer for a wrong reason is not marked sound" {
            let grade = marked(
                "It compiles because `int` and `1` are the same type; the number is just how you write an int.",
            );
            assert.is_true(grade.understanding != root.engine.Understanding.Sound)
        }
    } else {
        log.warn("live::grader not registered: ANTHROPIC_API_KEY is unset")
    }
}
