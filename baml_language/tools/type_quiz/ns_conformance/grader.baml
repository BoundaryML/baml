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

/// The judge the fixtures ask: a key, and the model it can be spent on.
class Live {
    /// Empty when no provider's key is in the environment, which is what
    /// keeps these fixtures off every run that did not ask for them.
    key: string,
    model: string,
}

/// The value of `name`, or empty when it is unset or unreadable.
function env_key(name: string) -> string throws never {
    baml.env.get(name) catch_all (e) {
        baml.errors.Io => "",
        baml.errors.ParseError => "",
    }
        ?? ""
}

/// One provider's judge: its key from the environment, and the best model the
/// engine's own table offers for it, or `null` when the key is unset. The
/// model is looked up rather than named here, so these fixtures cannot ask
/// for a model the judge does not offer.
function live_for(name: string, provider: root.engine.Provider) -> Live? throws never {
    let key = env_key(name);
    if (key == "") {
        return null;
    }
    let offer = root.engine.offered().find((offer) -> {
        offer.provider == provider
    })
        ?? return null;
    Live { key, model: offer.id }
}

/// Whichever provider's key is set, Anthropic first. Calibration is about the
/// grader's prompt and rubric rather than about one provider, so it runs on
/// whichever key the person asking for it has.
function live_judge() -> Live throws never {
    live_for("ANTHROPIC_API_KEY", root.engine.Provider.Anthropic)
        ?? live_for("OPENAI_API_KEY", root.engine.Provider.OpenAI)
        ?? Live { key: "", model: "" }
}

/// A provider error is a failed fixture, not something to absorb: these run
/// only when someone has asked for them with a key, and then they should say
/// what went wrong.
function marked(reasoning: string) -> root.engine.Grade {
    let judge = live_judge();
    let judged = root.engine.judge_reasoning(judge.key, judge.model, fixture_revealed(), "compiles", reasoning);
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
    if (live_judge().key != "") {
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
        log.warn("live::grader not registered: neither ANTHROPIC_API_KEY nor OPENAI_API_KEY is set")
    }
}
