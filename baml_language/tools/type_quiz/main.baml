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
    /// Whether the learner said the case compiles.
    compiles: bool,
    /// What they said it turns on. Empty when they were not asked.
    reasoning: string,
    /// How they marked their own reasoning: `sound`, `partial`, `wrong`, or
    /// empty when they did not mark it.
    mark: string,
}

function verdict_given(given: Given) -> root.engine.VerdictKind throws never {
    if (given.compiles) {
        root.engine.VerdictKind.Compiles
    } else {
        root.engine.VerdictKind.Rejected
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
