// The composition root, and the surface a user interface calls.
//
// This is the only file that knows both the engine and the bank. Everything
// here is a top-level function, which is what the generated SDKs export, and
// every one of them is a pure function of its arguments except `review`,
// which reads a file.
//
// A sitting is identified by its seed and its length, and a question by its
// index within one. Nothing is held between calls: the same seed plans the
// same sitting every time, so asking again is cheaper than remembering, and a
// learner is never handed the answer along with the question.
//
// BUG: nothing crossing INTO this surface may be an enum, nor a class holding
// one. The TypeScript web bridge encodes an enum member as a bare string,
// because a TypeScript string enum's member *is* a string and carries nothing
// to tell it apart from one. The engine puts that string in a slot typed as
// the enum, which then fails two different ways: a `match` on it panics with
// `VM internal error: type error: expected variant, got string`, and an `==`
// against a variant quietly answers false. The second is the dangerous one --
// a quiz that marked every answer wrong, with no error anywhere. So a
// learner's answer arrives as `Given`, whose fields are a bool and two
// strings, and this file turns it into the enums the engine speaks in. Enums
// coming BACK are fine: the bridge resolves those through the generated type
// map.

function bank_items() -> root.engine.Item[] throws never {
    root.bank.items()
}

function question_at(seed: int, length: int, index: int) -> root.engine.Question? throws never {
    root.engine.question_at(
        bank_items(),
        root.engine.seed_from(seed),
        root.bank.default_weights(),
        length,
        root.bank.default_budget(),
        index,
    )
}

/// How many questions a sitting actually has. Usually `length`; fewer only if
/// the bank could not fill it.
function sitting_length(seed: int, length: int) -> int throws never {
    root.engine
        .plan(
            bank_items(),
            root.engine.seed_from(seed),
            root.bank.default_weights(),
            length,
            root.bank.default_budget(),
        )
        .length()
}

/// The `index`-th program of a sitting, with no answer attached, or `null`
/// past the end.
function prompt_at(seed: int, length: int, index: int) -> root.engine.Prompt? throws never {
    let question = question_at(seed, length, index)
        ?? return null;
    root.engine.prompt_of(question)
}

/// What a learner said about one question.
///
/// Every field is a bool or a string, and deliberately so: see the note at the
/// top of this file on what an enum becomes on its way in.
class Given {
    /// What the learner said the compiler does: `compiles`, `rejected`, or
    /// `unsure` when they would not commit.
    said: string,
    /// What they said it turns on. Empty when they were not asked.
    reasoning: string,
    /// How they marked their own reasoning: `sound`, `partial`, `wrong`, or
    /// empty when they did not mark it.
    mark: string,
}

/// `given.said` as the engine's verdict, or `null` when the learner abstained.
///
/// # Panics
///
/// - `baml.panics.Unreachable` if the word is not one this surface offers, for
///   the same reason as `marked_given`.
function verdict_given(given: Given) -> root.engine.VerdictKind? throws never {
    if (given.said == "compiles") {
        root.engine.VerdictKind.Compiles
    } else if (given.said == "rejected") {
        root.engine.VerdictKind.Rejected
    } else if (given.said == "unsure") {
        null
    } else {
        throw baml.panics.Unreachable { message: `no such answer: ${given.said}` }
    }
}

/// `given.mark` as the engine's own word for it, or `null` when unmarked.
///
/// # Panics
///
/// - `baml.panics.Unreachable` if the mark is not one this surface offers. The
///   caller is an interface of ours rather than a stranger, so a mark it never
///   showed anyone is a defect in it and not something to absorb.
function marked_given(given: Given) -> root.engine.Reasoning? throws never {
    let understanding = if (given.mark == "") {
        return null;
    } else if (given.mark == "sound") {
        root.engine.Understanding.Sound
    } else if (given.mark == "partial") {
        root.engine.Understanding.Partial
    } else if (given.mark == "wrong") {
        root.engine.Understanding.Wrong
    } else {
        throw baml.panics.Unreachable { message: `no such mark: ${given.mark}` }
    };
    root.engine.Reasoning { understanding, marked_by: null }
}

/// Answer the `index`-th question: the case is worked out again from the
/// seed, the verdict is marked against its key, and what comes back is the
/// whole exchange, including the derivation to show the learner.
function answer_at(
    seed: int,
    length: int,
    index: int,
    given: Given,
) -> root.engine.Exchange? throws never {
    let question = question_at(seed, length, index)
        ?? return null;
    let answer = root.engine.Answer { verdict: verdict_given(given), reasoning: given.reasoning };
    root.engine.Exchange {
        question,
        answer,
        judgement: root.engine.judge(question, answer, marked_given(given)),
    }
}

/// A sitting rebuilt from what the learner said about each question in turn.
///
/// The interface keeps only the answers; every judged value is worked out
/// here, so there is one account of how a sitting went rather than one on each
/// side of the boundary.
function sitting_of(
    seed: int,
    length: int,
    full: bool,
    answers: Given[],
) -> root.engine.Transcript throws never {
    let mode = if (full) {
        root.engine.Mode.Full
    } else {
        root.engine.Mode.Simple
    };
    let exchanges = baml.iter.Range.new(0, answers.length())
        .filter_map((index) -> {
            answer_at(seed, length, index, answers[index])
        })
        .collect();
    root.engine.Transcript { mode, seed, exchanges }
}

/// How a sitting has gone so far.
function sitting_score(
    seed: int,
    length: int,
    full: bool,
    answers: Given[],
) -> root.engine.Score throws never {
    root.engine.tally(sitting_of(seed, length, full, answers))
}

/// What the compiler says about the `index`-th case, in its own words. For
/// showing beside the derivation once the learner has answered.
function compiler_report(seed: int, length: int, index: int) -> root.engine.Reported? throws never {
    let question = question_at(seed, length, index)
        ?? return null;
    root.engine.report(question.case)
}

/// A finished sitting as the JSON a learner takes away. `review` reads back
/// exactly this.
function sitting_json(seed: int, length: int, full: bool, answers: Given[]) -> string throws never {
    baml.json.to_string(sitting_of(seed, length, full, answers)) catch_all (e) {
        baml.json.SerializationError => `{"error":"${e}"}`,
    }
}

/// Read a sitting back from a file: how it went, and whether the compiler
/// still says what the transcript recorded. Run from a terminal, over a
/// transcript downloaded from the quiz.
function review(path: string) -> string throws never {
    match (root.engine.load(path)) {
        let failed: root.engine.LoadFailed => `could not read ${failed.path}: ${failed.message}`,
        let loaded: root.engine.Loaded => read_back(loaded.transcript),
    }
}

function read_back(transcript: root.engine.Transcript) -> string throws never {
    let score = root.engine.tally(transcript);
    let stale = transcript.exchanges.filter_map((exchange) -> {
        let outcome = root.engine.verify(exchange.question.case);
        if (root.engine.agrees(outcome)) {
            null
        } else {
            `  ${exchange.question.item} (seed ${exchange.question.seed}): ${outcome}`
        }
    });
    let standing = if (stale.length() == 0) {
        "every case still behaves as recorded"
    } else {
        `cases that no longer behave as recorded:\n${stale.join("\n")}`
    };
    `${score.verdicts_right} of ${score.asked} verdicts right; reasoning ${score.sound} sound, ${score.partial} partial, ${score.wrong} wrong, ${score.unmarked} unmarked\n${standing}`
}

// ─── the adaptive sitting ────────────────────────────────────────────────────
//
// The default mode: no fixed length, and no answer key on the page. The page
// holds a `Profile` (plain data, so it survives the boundary and browser
// storage alike) and hands it back with every answer; the engine decides what
// to ask next and when to stop.

/// A profile that has seen nothing, over every rule the bank can conclude a
/// case with and every naive model it knows.
function fresh_profile() -> root.engine.Profile throws never {
    root.engine.fresh(
        root.bank.rule_orders(4),
        root.bank.models().map((model) -> {
            model.id
        }),
    )
}

/// The case behind a prompt, generated again from its item and seed, or
/// `null` when no such item exists.
function candidate_of(item: string, seed: int) -> root.engine.Candidate? throws never {
    let found = bank_items().find((candidate) -> {
        candidate.id() == item
    })
        ?? return null;
    root.engine.generate(found, root.engine.Seed { state: seed })
}

/// The rules a case holds as the tracer reads them: its trace, inner rule
/// first, without the flow rule every probe ends in.
function tracer_rules(candidate: root.engine.Candidate) -> string[] throws never {
    let flow = root.bank.flow_requires_subtype().name;
    candidate.case.trace
        .map((claim) -> {
            claim.rule.name
        })
        .filter((name) -> {
            name != flow
        })
}

/// The next case to put to a learner, and the rule to teach first if one is
/// stuck.
class Next {
    prompt: root.engine.Prompt,
    teach: root.engine.Rule?,
}

/// The case the model would ask next: a draw of candidates from the verified
/// items at a seed of the session and step, scored for this learner, and
/// picked at random in proportion. `null` when the bank has nothing to draw.
function next_prompt(
    profile: root.engine.Profile,
    knobs: root.engine.Knobs,
    session: int,
    step: int,
) -> Next? throws never {
    let seed = root.engine.seed_for(`mastery/${session}`, step);
    let streams = root.engine.split(seed);
    let verified = bank_items().filter((item) -> {
        item.status() is root.engine.Verified
    });
    let reads = root.engine
        .batch(verified, streams.left, root.bank.default_weights(), root.bank.default_budget())
        .map((scored) -> {
            root.engine.Read { scored, rules: tracer_rules(scored.candidate) }
        });
    let chosen = root.engine.choose(profile, knobs, reads, streams.right)
        ?? return null;
    let question = root.engine.Question {
        item: chosen.scored.item,
        seed: chosen.scored.seed.state,
        case: chosen.scored.candidate.case,
    };
    let teach = match (root.engine.to_teach(profile)) {
        let name: string => {
            root.bank.rules().find((rule) -> {
                rule.name == name
            })
        },
        null => null,
    };
    Next { prompt: root.engine.prompt_of(question), teach }
}

/// The profile after the learner answered the case at `item` and `seed`.
///
/// # Panics
///
/// - `baml.panics.Unreachable` if there is no such case: the page only hands
///   back what `next_prompt` gave it, so a missing case is a defect in the
///   page rather than something to absorb.
function observe_answer(
    profile: root.engine.Profile,
    knobs: root.engine.Knobs,
    item: string,
    seed: int,
    given: Given,
) -> root.engine.Profile throws never {
    let candidate = candidate_of(item, seed)
        ?? throw baml.panics.Unreachable { message: `no case for ${item} at seed ${seed}` };
    let verdict = match (verdict_given(given)) {
        let said: root.engine.VerdictKind => {
            said == root.engine.verdict_kind(candidate.case.key)
        },
        null => null,
    };
    root.engine.observe(
        profile,
        knobs,
        root.engine.Observed { rules: tracer_rules(candidate), traps: candidate.traps, verdict },
    )
}
