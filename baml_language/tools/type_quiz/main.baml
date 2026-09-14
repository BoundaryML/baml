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
    /// What the learner said. Of one program: `compiles` or `rejected`. Of
    /// two put side by side: `first` or `second`, naming the one they say
    /// the compiler accepts. Either way `unsure` when they would not commit.
    said: string,
    /// What they said it turns on. Empty when they were not asked.
    reasoning: string,
    /// How the reasoning was marked: `sound`, `partial`, `wrong`, or empty
    /// when it was not marked.
    mark: string,
    /// Who marked it: the model's name when the grader did, empty when the
    /// learner marked their own against the explanation they were shown.
    marked_by: string,
}

/// `given.said` as what the engine reads, or `null` when the learner
/// abstained. Whether the word suits the question they were asked is the
/// judge's to say, not this function's.
///
/// # Panics
///
/// - `baml.panics.Unreachable` if the word is not one this surface offers, for
///   the same reason as `marked_given`.
function said_given(given: Given) -> root.engine.Said? throws never {
    root.engine.said_of(given.said)
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
    let marked_by = if (given.marked_by == "") {
        null
    } else {
        given.marked_by
    };
    root.engine.Reasoning { understanding, marked_by }
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
    let answer = root.engine.Answer { said: said_given(given), reasoning: given.reasoning };
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
    root.engine.Transcript { mode: mode_of(full), seed, exchanges, learner: null, built: null }
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
///
/// # Panics
///
/// - `baml.panics.Unreachable` if the question put two programs side by side.
///   A sitting planned from a seed alone puts one at a time, and this is the
///   surface for such a sitting.
function compiler_report(seed: int, length: int, index: int) -> root.engine.Reported? throws never {
    let question = question_at(seed, length, index)
        ?? return null;
    match (question.posed) {
        let case: root.engine.Case => root.engine.report(case),
        root.engine.Choice => {
            throw baml.panics.Unreachable { message: "a sitting planned from a seed alone puts one program at a time" }
        },
    }
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
    let stale = transcript.exchanges.flat_map((exchange) -> {
        root.engine.cases_of(exchange.question.posed).filter_map((case) -> {
            let outcome = root.engine.verify(case);
            if (root.engine.agrees(outcome)) {
                null
            } else {
                `  ${exchange.question.item} (seed ${exchange.question.seed}): ${outcome}`
            }
        })
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
    let built = match (transcript.built) {
        let commit: string => `\nsat against the bank built at ${commit}`,
        null => "",
    };
    `${score.answers_right} of ${score.asked} answers right; reasoning ${score.sound} sound, ${score.partial} partial, ${score.wrong} wrong, ${score.unmarked} unmarked\n${standing}${learned}${built}`
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

/// The item called `item`.
///
/// # Panics
///
/// - `baml.panics.Unreachable` if the bank has no such item. A page only
///   hands back what it was given, so a name the bank does not know is a
///   defect in the page, or a sitting saved against a bank that has since
///   moved on, and neither is something to absorb.
function item_named(item: string) -> root.engine.Item throws never {
    bank_items().find((candidate) -> {
        candidate.id() == item
    })
        ?? throw baml.panics.Unreachable { message: `no item called ${item}` }
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

/// The rules a posed question holds as the tracer reads them: its trace,
/// inner rule first, without the rule the site itself contributed.
function tracer_rules(posed: root.engine.Posed) -> string[] throws never {
    root.engine.rules_of(posed).filter((name) -> {
        !root.bank.is_site_rule(name)
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
            root.engine.Read { scored, rules: tracer_rules(scored.candidate.case) }
        });
    let chosen = root.engine.choose(profile, knobs, reads, streams.right)
        ?? return null;
    let put = put_at(knobs, chosen.scored.item, chosen.scored.seed.state * 1n);
    let question = root.engine.Question {
        item: chosen.scored.item,
        seed: chosen.scored.seed.state,
        posed: put.posed,
    };
    // The stuck rule is taught only before a case that turns on it: the
    // selection leans toward such a case but does not insist, and the spec's
    // words on one rule shown before a case about another would mislead.
    let held = tracer_rules(put.posed);
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

/// How the case at `item` and `seed` is put under `knobs`: what a learner
/// would be shown and asked.
///
/// The answer travels with it, so this is for something that already holds
/// the key — a test, or a tool reading a sitting back. What a page is handed
/// is a `Prompt`, which carries neither.
///
/// # Panics
///
/// - As `case_at`.
function put_at(
    knobs: root.engine.Knobs,
    item: string,
    seed: bigint,
) -> root.engine.Put throws never {
    let found = item_named(item);
    let state = root.engine.Seed { state: state_of(seed) };
    let candidate = root.engine.generate(found, state)
        ?? throw baml.panics.Unreachable { message: `no case for ${item} at seed ${seed}` };
    root.engine.put(
        candidate,
        () -> {
            // Through the free function: an interface method on a value the
            // closure captured dies at emit time (README, compiler issue 1).
            root.engine.foil_of(found, state)
        },
        knobs,
        root.engine.pose_seed(state_of(seed)),
    )
}

/// A case put to a learner and their answer judged against it. Everything
/// that follows from an answer follows from this one value, so how the case
/// was put, what it traps, and whether the answer was right cannot come
/// apart from each other.
class Judged {
    put: root.engine.Put,
    exchange: root.engine.Exchange,
}

/// The case at `item` and `seed`, put as the knobs say and judged.
///
/// # Panics
///
/// - As `case_at`.
function judged(
    knobs: root.engine.Knobs,
    item: string,
    seed: bigint,
    given: Given,
) -> Judged throws never {
    let put = put_at(knobs, item, seed);
    let question = root.engine.Question { item, seed: state_of(seed), posed: put.posed };
    Judged { put, exchange: exchange_of(question, given) }
}

function observe_judged(
    profile: root.engine.Profile,
    knobs: root.engine.Knobs,
    judgement: Judged,
) -> root.engine.Profile throws never {
    root.engine.observe(
        profile,
        knobs,
        root.engine.Observed {
            item: judgement.exchange.question.item,
            rules: tracer_rules(judgement.put.posed),
            traps: judgement.put.traps,
            verdict: judgement.exchange.judgement.verdict_correct,
        },
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
    observe_judged(profile, knobs, judged(knobs, item, seed, given))
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
    /// Every program the question showed, with what the compiler makes of
    /// it, what they have in common and what tells them apart.
    revealed: root.engine.Revealed,
    /// What the answer was worth under the knobs' bar.
    points: int,
}

/// Answer the case at `item` and `seed`: the profile that follows, the
/// exchange to show, the reveal, and the points.
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
    let judgement = judged(knobs, item, seed, given);
    Answered {
        profile: observe_judged(profile, knobs, judgement),
        exchange: judgement.exchange,
        revealed: root.engine.reveal(judgement.put.posed),
        points: root.engine.points_for(knobs, judgement.exchange.judgement.verdict_correct),
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
    built: string?,
) -> root.engine.Transcript throws never {
    root.engine.Transcript {
        mode: mode_of(full),
        seed: session,
        exchanges: taken.map((t) -> {
            judged(knobs, t.item, t.seed, t.given).exchange
        }),
        learner: root.engine.Learned { knobs, profile: replay(knobs, taken) },
        built,
    }
}

/// A finished adaptive sitting as the JSON a learner takes away.
function adaptive_json(
    session: int,
    full: bool,
    knobs: root.engine.Knobs,
    taken: Taken[],
    built: string?,
) -> string throws never {
    baml.json.to_string(adaptive_sitting(session, full, knobs, taken, built)) catch_all (e) {
        baml.json.SerializationError => `{"error":"${e}"}`,
    }
}

// ─── driving a sitting from outside ──────────────────────────────────────────
//
// A driver that is not a browser — a script standing something else in for a
// learner — runs a sitting through `adaptive_step`. The whole state of a
// sitting is its session number and the answers given so far, in order,
// because everything else is a pure function of those two: which case each
// question draws and what the learner's profile is both follow. So a driver
// holds one string, and can stop and resume on it.
//
// What comes back is in two parts, and the split is the honest boundary.
// `asked` is everything a learner may see BEFORE answering: the programs,
// what separates two of them, and the rule to teach if they have stalled on
// one. It carries neither the key nor which item the case came from, so a
// driver cannot pass on what it does not have. `marked` is what the LAST
// answer was worth, which a learner may see only after giving it, exactly as
// the page shows it and for the same reason.

/// What a learner may see before they answer.
class Asked {
    /// The program to judge, or the two to choose between, in the order they
    /// are shown. Their file names are not part of the question.
    ///
    /// What separates two programs is NOT here. The page says that in the
    /// reveal, after the answer, and a driver that said it first would be
    /// asking an easier question than the page does.
    programs: string[],
    /// The spec's own words on a rule they have stalled on, to be read before
    /// this case; empty when there is nothing to teach.
    teach: string,
}

/// What the last answer was worth.
class Marked {
    said: string,
    /// Whether they were right, or null when they withheld.
    correct: bool?,
    points: int,
    /// Whether the compiler accepts each program shown, in order.
    compiles: bool[],
    /// What the compiler says about each program, in its own words.
    reported: string[],
    /// The rules the case turns on, instantiated at its types: why the
    /// answer is what it is.
    because: string[],
    /// What separated the two programs; empty when only one was shown.
    difference: string,
}

/// What a finished sitting concluded.
class Ended {
    why: string,
    steps: int,
    committed: int,
    right: int,
    mastered: string[],
    shaky: string[],
    /// How often the learner committed to an answer they did not have.
    commits_unsure: float?,
    withheld: float,
    suspected: string?,
}

/// One step of a driven sitting: what the last answer was worth, what to ask
/// next, and what the sitting concluded if it is over.
class Step {
    /// How many answers have been given.
    step: int,
    marked: Marked?,
    asked: Asked?,
    ended: Ended?,
}

/// A sitting replayed through `answers`: the questions it drew and the
/// profile it reached.
class Driven {
    taken: Taken[],
    profile: root.engine.Profile,
}

/// The answers of `answers`, one word each: `compiles`, `rejected`, `first`,
/// `second`, or `unsure`. An empty string is no answers at all.
function answers_of(answers: string) -> string[] throws never {
    answers.split(",")
        .filter((word) -> {
            word.trim() != ""
        })
        .map((word) -> {
            word.trim()
        })
}

/// The sitting drawn from `session` replayed through `answers`.
///
/// Selection is a pure function of the session and the answers before a
/// step, so replaying the answers recovers which case every question drew.
///
/// # Panics
///
/// - `baml.panics.Unreachable` if there are more answers than the sitting has
///   questions, which means they did not come from this sitting.
/// - As `said_given`, for a word this surface does not offer.
function driven(session: int, answers: string) -> Driven throws never {
    let knobs = root.engine.default_knobs();
    let taken: Taken[] = [];
    let profile = answers_of(answers).reduce(
        (carried, said) -> {
            let next = next_prompt(carried, knobs, session, taken.length())
                ?? throw baml.panics.Unreachable {
                    message: `the sitting from ${session} has no question ${taken.length()}`,
                };
            let given = Given { said, reasoning: "", mark: "", marked_by: "" };
            taken.push(Taken { item: next.prompt.item, seed: next.prompt.seed, given });
            observe_answer(carried, knobs, next.prompt.item, next.prompt.seed, given)
        },
        fresh_profile(),
    );
    Driven { taken, profile }
}

/// The programs of a prompt, in the order they are shown.
function programs_of(prompt: root.engine.Prompt) -> string[] throws never {
    let shown = [prompt.program];
    match (prompt.against) {
        let second: map<string, string> => shown.push(second),
        null => {},
    }
    shown.map((files) -> {
        root.engine.source_of(files)
    })
}

/// What the answer at the end of `answers` was worth.
function marked_last(knobs: root.engine.Knobs, taken: Taken[]) -> Marked? throws never {
    let last = taken.at(-1)
        ?? return null;
    let judgement = judged(knobs, last.item, last.seed, last.given);
    let revealed = root.engine.reveal(judgement.put.posed);
    Marked {
        said: last.given.said,
        correct: judgement.exchange.judgement.verdict_correct,
        points: root.engine.points_for(knobs, judgement.exchange.judgement.verdict_correct),
        compiles: revealed.programs.map((program) -> {
            program.compiles
        }),
        reported: revealed.programs.map((program) -> {
            let said = program.reported.codes.concat(program.reported.messages);
            said.join("; ")
        }),
        because: revealed.shared.map((claim) -> {
            claim.instance
        }),
        difference: revealed.difference,
    }
}

function why_said(why: root.engine.Why) -> string throws never {
    match (why) {
        root.engine.Why.Continue => "continue",
        root.engine.Why.Mastered => "mastered",
        root.engine.Why.Budget => "budget",
        root.engine.Why.Stalled => "stalled",
    }
}

/// One step of a sitting driven from outside: hand it every answer given so
/// far and it says what the last one was worth and what to ask next.
///
/// # Panics
///
/// - As `driven`.
function adaptive_step(session: int, answers: string) -> string throws never {
    let knobs = root.engine.default_knobs();
    let sat = driven(session, answers);
    let standing = root.engine.standing(sat.profile, knobs);
    let over = standing.why != root.engine.Why.Continue;
    let asked = if (over) {
        null
    } else {
        let next = next_prompt(sat.profile, knobs, session, sat.taken.length());
        match (next) {
            let ask: Next => {
                Asked {
                    programs: programs_of(ask.prompt),
                    teach: match (ask.teach) {
                        let rule: root.engine.Rule => rule.statement(),
                        null => "",
                    },
                }
            },
            null => null,
        }
    };
    let step = Step {
        step: sat.taken.length(),
        marked: marked_last(knobs, sat.taken),
        asked,
        ended: if (over) {
            Ended {
                why: why_said(standing.why),
                steps: sat.profile.step,
                committed: sat.profile.committed,
                right: sat.profile.right,
                mastered: standing.mastered,
                shaky: standing.shaky,
                commits_unsure: standing.commits_unsure,
                withheld: standing.withheld,
                suspected: standing.suspected,
            }
        } else {
            null
        },
    };
    baml.json.to_string(step) catch_all (e) {
        baml.json.SerializationError => `{"error":"${e}"}`,
    }
}

/// A driven sitting as the JSON a learner would have taken away.
///
/// # Panics
///
/// - As `driven`.
function adaptive_record(session: int, answers: string) -> string throws never {
    let knobs = root.engine.default_knobs();
    adaptive_json(session, false, knobs, driven(session, answers).taken, null)
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
