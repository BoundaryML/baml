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
    exchange_of(question, given)
}

/// The whole exchange for what was said about a question: the answer as the
/// engine speaks it, marked against the key.
function exchange_of(
    question: root.engine.Question,
    given: Given,
) -> root.engine.Exchange throws never {
    let answer = root.engine.Answer { verdict: verdict_given(given), reasoning: given.reasoning };
    root.engine.Exchange {
        question,
        answer,
        judgement: root.engine.judge(question, answer, marked_given(given)),
    }
}

function mode_of(full: bool) -> root.engine.Mode throws never {
    if (full) {
        root.engine.Mode.Full
    } else {
        root.engine.Mode.Simple
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
    let exchanges = baml.iter.Range.new(0, answers.length())
        .filter_map((index) -> {
            answer_at(seed, length, index, answers[index])
        })
        .collect();
    root.engine.Transcript { mode: mode_of(full), seed, exchanges, learner: null }
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
    let learned = match (transcript.learner) {
        let learned: root.engine.Learned => concluded(learned),
        null => "",
    };
    `${score.verdicts_right} of ${score.asked} verdicts right; reasoning ${score.sound} sound, ${score.partial} partial, ${score.wrong} wrong, ${score.unmarked} unmarked\n${standing}${learned}`
}

/// What the model concluded about the learner, as a line of the review.
function concluded(learned: root.engine.Learned) -> string throws never {
    let standing = root.engine.standing(learned.profile, learned.knobs);
    let shaky = standing.shaky.slice(0, 5).join(", ");
    let suspected = standing.suspected ?? "none";
    `\nmastered ${standing.mastered.length()} of ${learned.profile.skills.length()} rules, ended ${standing.why}; shakiest: ${shaky}; reasoning suspected of: ${suspected}`
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

/// A stream state a page handed back, narrowed to the `int` it came from.
///
/// # Panics
///
/// - `baml.panics.Unreachable` if it does not fit: every state a page is given
///   is an `int`, so one that is not is a defect in the page.
function state_of(seed: bigint) -> int throws never {
    seed.to_int() catch (e) {
        baml.errors.InvalidArgument => {
            throw baml.panics.Unreachable { message: `${seed} is not a stream state` }
        },
    }
}

/// The case behind a prompt, generated again from its item and seed, or
/// `null` when no such item exists.
function candidate_of(item: string, seed: bigint) -> root.engine.Candidate? throws never {
    let found = bank_items().find((candidate) -> {
        candidate.id() == item
    })
        ?? return null;
    root.engine.generate(found, root.engine.Seed { state: state_of(seed) })
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

/// The next case to put to a learner, and the rule to teach before it when
/// the case turns on one the learner is stuck on.
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
    // The stuck rule is taught only before a case that turns on it: the
    // selection leans toward such a case but does not insist, and the spec's
    // words on one rule shown before a case about another would mislead.
    let held = tracer_rules(chosen.scored.candidate);
    let teach = match (root.engine.to_teach(profile)) {
        let name: string => {
            root.bank.rules().find((rule) -> {
                rule.name == name && held.includes(name)
            })
        },
        null => null,
    };
    Next { prompt: root.engine.prompt_of(question), teach }
}

/// The case behind a prompt, generated again from its item and seed.
///
/// # Panics
///
/// - `baml.panics.Unreachable` if there is no such case. The page only hands
///   back what `next_prompt` gave it, so a missing case is a defect in the
///   page, or a saved sitting from a bank that no longer has the item, and
///   neither is something to absorb.
function case_at(item: string, seed: bigint) -> root.engine.Candidate throws never {
    candidate_of(item, seed)
        ?? throw baml.panics.Unreachable { message: `no case for ${item} at seed ${seed}` }
}

function question_for(
    item: string,
    seed: bigint,
    candidate: root.engine.Candidate,
) -> root.engine.Question throws never {
    root.engine.Question { item, seed: state_of(seed), case: candidate.case }
}

function observe_candidate(
    profile: root.engine.Profile,
    knobs: root.engine.Knobs,
    candidate: root.engine.Candidate,
    given: Given,
) -> root.engine.Profile throws never {
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

/// The profile after the learner answered the case at `item` and `seed`.
///
/// # Panics
///
/// - As `case_at`.
function observe_answer(
    profile: root.engine.Profile,
    knobs: root.engine.Knobs,
    item: string,
    seed: bigint,
    given: Given,
) -> root.engine.Profile throws never {
    observe_candidate(profile, knobs, case_at(item, seed), given)
}

// ─── what a page keeps ───────────────────────────────────────────────────────
//
// A page holds, for each question of an adaptive sitting, where the case
// came from and what the learner said, and nothing else. The profile, the
// judgements, the transcript and the readout are all worked out again from
// those, so a saved sitting is as small as it can be and can never disagree
// with itself.

/// One question of an adaptive sitting, as the page keeps it.
class Taken {
    item: string,
    /// As a `Prompt` carries it.
    seed: bigint,
    given: Given,
}

/// Everything that follows from one answer, worked out together so the case
/// is generated once and there is one account of what happened.
class Answered {
    profile: root.engine.Profile,
    exchange: root.engine.Exchange,
    /// What the compiler itself says about the case, in its own words.
    reported: root.engine.Reported,
    /// What the answer was worth under the knobs' bar.
    points: int,
}

/// Answer the case at `item` and `seed`: the profile that follows, the
/// exchange to show, the compiler's own words, and the points.
///
/// # Panics
///
/// - As `case_at`.
function answer_case(
    profile: root.engine.Profile,
    knobs: root.engine.Knobs,
    item: string,
    seed: bigint,
    given: Given,
) -> Answered throws never {
    let candidate = case_at(item, seed);
    let exchange = exchange_of(question_for(item, seed, candidate), given);
    Answered {
        profile: observe_candidate(profile, knobs, candidate, given),
        exchange,
        reported: root.engine.report(candidate.case),
        points: root.engine.points_for(knobs, exchange.judgement.verdict_correct),
    }
}

/// The profile after every answer in `taken`, from nothing. A page loading a
/// saved sitting rebuilds its profile with this rather than reading one back,
/// so the profile is never stored beside the answers it follows from, where
/// the two could drift apart.
///
/// # Panics
///
/// - As `case_at`, for any question the bank no longer has.
function replay(knobs: root.engine.Knobs, taken: Taken[]) -> root.engine.Profile throws never {
    taken.reduce(
        (profile, t) -> {
            observe_answer(profile, knobs, t.item, t.seed, t.given)
        },
        fresh_profile(),
    )
}

/// An adaptive sitting rebuilt from what the page kept, with what the model
/// concluded about the learner riding along. `review` reads back exactly
/// this.
///
/// # Panics
///
/// - As `replay`.
function adaptive_sitting(
    session: int,
    full: bool,
    knobs: root.engine.Knobs,
    taken: Taken[],
) -> root.engine.Transcript throws never {
    root.engine.Transcript {
        mode: mode_of(full),
        seed: session,
        exchanges: taken.map((t) -> {
            exchange_of(question_for(t.item, t.seed, case_at(t.item, t.seed)), t.given)
        }),
        learner: root.engine.Learned { knobs, profile: replay(knobs, taken) },
    }
}

/// A finished adaptive sitting as the JSON a learner takes away.
function adaptive_json(
    session: int,
    full: bool,
    knobs: root.engine.Knobs,
    taken: Taken[],
) -> string throws never {
    baml.json.to_string(adaptive_sitting(session, full, knobs, taken)) catch_all (e) {
        baml.json.SerializationError => `{"error":"${e}"}`,
    }
}

/// What a learner who reasons as the naive model `id` does believes, in the
/// second person; `null` for a model the bank does not know.
function describe_model(id: string) -> string? throws never {
    root.bank.models()
        .find((model) -> {
            model.id == id
        })
        ?.description
}
