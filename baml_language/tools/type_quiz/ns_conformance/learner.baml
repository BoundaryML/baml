// Conformance suite for the learner model: scripted learners sit the quiz
// through the same surface a page uses, and what the model concludes about
// them must match who they are. An expert is certified well under the budget,
// a learner who never commits is never certified, a learner who reasons as
// one naive model does is found out as that model, the points follow the bar,
// and a profile survives the trip to JSON.

// ─── scripted learners ───────────────────────────────────────────────────────

/// A learner is a rule for answering a question whose key it can see: the
/// word it would say.
///
/// A learner answers the question it is asked rather than the program it is
/// shown, because the two are not the same question: a program alone is
/// asked for its verdict, and two side by side for which of them the
/// compiler accepts. `right_word` and `wrong_word` say what those words are,
/// so each learner below is the one sentence that describes it.
type Learner = (root.engine.Put) -> string throws never;

function key_compiles(case: root.engine.Case) -> bool throws never {
    root.engine.verdict_kind(case.key) == root.engine.VerdictKind.Compiles
}

/// The answer that is right, in the words the question takes.
function right_word(put: root.engine.Put) -> string throws never {
    match (put.posed) {
        let case: root.engine.Case => {
            if (key_compiles(case)) {
                "compiles"
            } else {
                "rejected"
            }
        },
        let choice: root.engine.Choice => {
            if (choice.second_compiles()) {
                "second"
            } else {
                "first"
            }
        },
    }
}

/// The other answer: the one committed answer that is wrong.
function wrong_word(put: root.engine.Put) -> string throws never {
    match (put.posed) {
        let case: root.engine.Case => {
            if (key_compiles(case)) {
                "rejected"
            } else {
                "compiles"
            }
        },
        let choice: root.engine.Choice => {
            if (choice.second_compiles()) {
                "first"
            } else {
                "second"
            }
        },
    }
}

function expert(put: root.engine.Put) -> string throws never {
    right_word(put)
}

function silent(put: root.engine.Put) -> string throws never {
    "unsure"
}

/// Answers exactly as the TypeScript model would: right unless the question
/// traps it.
function typescripter(put: root.engine.Put) -> string throws never {
    if (put.traps.includes("typescript")) {
        wrong_word(put)
    } else {
        right_word(put)
    }
}

/// Answers by a bit that has nothing to do with the question: whether the
/// first program shown has an even number of bytes.
function parity(put: root.engine.Put) -> string throws never {
    let shown = root.engine.cases_of(put.posed);
    let bytes = shown.at(0)?.files?.get("case.baml")?.length() ?? 0;
    if (bytes % 2 == 0) {
        right_word(put)
    } else {
        wrong_word(put)
    }
}

/// Knows the axioms cold and nothing about variance: right on one-rule
/// questions, withholds on the rest.
function beginner(put: root.engine.Put) -> string throws never {
    if (root.tracer_rules(put.posed).length() <= 1) {
        right_word(put)
    } else {
        "unsure"
    }
}

/// One sitting: the learner answers whatever the model serves until the
/// model says the sitting is over.
class Sat {
    profile: root.engine.Profile,
    standing: root.engine.Standing,
    served: string[],
    /// What a page would have kept.
    taken: root.Taken[],
    /// How many cases came with a rule to teach, and how many of those
    /// cases did not turn on the rule taught.
    taught: int,
    taught_off_point: int,
    /// How many questions put two programs side by side.
    choices: int,
}

function sit(learner: Learner, knobs: root.engine.Knobs, session: int) -> Sat throws never {
    let start = root.fresh_profile();
    sit_from(learner, knobs, session, start, 0, [], 0, 0, 0)
}

function sit_from(
    learner: Learner,
    knobs: root.engine.Knobs,
    session: int,
    profile: root.engine.Profile,
    step: int,
    taken: root.Taken[],
    taught: int,
    taught_off_point: int,
    choices: int,
) -> Sat throws never {
    let served = taken.map((t) -> {
        t.item
    });
    let standing = root.engine.standing(profile, knobs);
    if (standing.why != root.engine.Why.Continue) {
        return Sat { profile, standing, served, taken, taught, taught_off_point, choices };
    }
    let next = root.next_prompt(profile, knobs, session, step)
        ?? return Sat { profile, standing, served, taken, taught, taught_off_point, choices };
    let put = root.put_at(knobs, next.prompt.item, next.prompt.seed);
    let chosen_now = if (put.posed is root.engine.Choice) {
        1
    } else {
        0
    };
    let off_point = match (next.teach) {
        let rule: root.engine.Rule => {
            if (root.tracer_rules(put.posed).includes(rule.name)) {
                0
            } else {
                1
            }
        },
        null => 0,
    };
    let taught_now = if (next.teach != null) {
        1
    } else {
        0
    };
    let given = root.Given { said: learner(put), reasoning: "", mark: "" };
    let after = root.observe_answer(profile, knobs, next.prompt.item, next.prompt.seed, given);
    let kept = root.Taken { item: next.prompt.item, seed: next.prompt.seed, given };
    sit_from(
        learner,
        knobs,
        session,
        after,
        step + 1,
        taken.concat([kept]),
        taught + taught_now,
        taught_off_point + off_point,
        choices + chosen_now,
    )
}

// ─── what the model concludes ────────────────────────────────────────────────

function expert_report() -> string throws never {
    let knobs = root.engine.default_knobs();
    let sat = sit(expert, knobs, 1);
    let calibration = sat.standing.calibration ?? 0.0;
    let suspected = sat.standing.suspected ?? "none";
    `why=${sat.standing.why} within_budget=${sat.profile.step < knobs.budget} withheld=${sat.standing.withheld} calibrated=${calibration >= 0.95} suspected=${suspected}`
}

/// Practice asks a fixed number of cases and stops nowhere else: the expert
/// is kept going well past the point at which certification would have let
/// them stop.
function practice_report() -> string throws never {
    let knobs = root.engine.default_knobs();
    let certified = sit(expert, knobs, 1);
    let count = certified.profile.step + 10;
    let practised = sit(expert, root.engine.Knobs { ...knobs, practice: true, budget: count }, 1);
    `certified=${certified.standing.why} practised=${practised.standing.why} asked_all=${practised.profile.step == count}`
}

function silent_report() -> string throws never {
    let sat = sit(silent, root.engine.default_knobs(), 2);
    `why=${sat.standing.why} mastered=${sat.standing.mastered.length()} committed=${sat.profile.committed}`
}

/// A strict sitting asks for every rule; a loose one stops on the overall
/// picture. The expert should be done sooner under loose than strict, and
/// both well within the budget.
function coverage_report() -> string throws never {
    let knobs = root.engine.default_knobs();
    let loose = sit(expert, knobs, 3);
    let strict = sit(expert, root.engine.Knobs { ...knobs, strict: true, budget: 200 }, 3);
    `loose_first=${loose.profile.step <= strict.profile.step} loose_done=${loose.standing.why} strict_done=${strict.standing.why}`
}

/// Every model's z-score in tenths, highest first.
function leading(profile: root.engine.Profile) -> string throws never {
    profile.suspicions.slice(0, profile.suspicions.length())
        .sort_by_key((s) -> {
            0.0 - root.engine.suspicion_z(s)
        })
        .map((s) -> {
            `${s.model}=${tenths(root.engine.suspicion_z(s))}`
        })
        .join(",")
}

function tenths(x: float) -> int throws never {
    (x * 10.0).iround() catch (e) {
        baml.errors.InvalidArgument => 0,
    }
}

/// The standing names the model, and it stands well clear of the covariant
/// model, which TypeScript's beliefs include and so is partly supported too.
/// A learner who keeps missing the same rules is taught, and every rule
/// taught is one the case before which it is taught turns on. A sitting long
/// enough to certify anyone puts both kinds of question.
function typescripter_report() -> string throws never {
    let sat = sit(typescripter, root.engine.default_knobs(), 4);
    let named = sat.standing.suspected ?? "none";
    `suspected=${named} leading=${leading(sat.profile)} taught=${sat.taught > 0} on_point=${sat.taught_off_point == 0} both_kinds=${sat.choices > 0 && sat.choices < sat.profile.step}`
}

/// The beginner is certified on nothing above order one, and the rules left
/// shaky are the variance rules.
function beginner_report() -> string throws never {
    let sat = sit(beginner, root.engine.default_knobs(), 5);
    let variance = sat.standing.mastered.filter((rule) -> {
        rule.starts_with("variance/")
    });
    let axioms = sat.standing.mastered.filter((rule) -> {
        !rule.starts_with("variance/")
    });
    `certified=${sat.standing.why == root.engine.Why.Mastered} variance_mastered=${variance.length()} some_axioms=${axioms.length() > 0} withheld_some=${sat.standing.withheld > 0.0}`
}

test "learner: an expert is certified well within the budget, and calibrated" {
    assert.equal(
        expert_report(),
        "why=Mastered within_budget=true withheld=0.0 calibrated=true suspected=none",
    )
}

test "learner: practice asks its count and stops nowhere else" {
    assert.equal(practice_report(), "certified=Mastered practised=Budget asked_all=true")
}

test "learner: a learner who never commits is never certified" {
    assert.equal(silent_report(), "why=Stalled mastered=0 committed=0")
}

test "learner: strict coverage costs more questions than an overall picture" {
    assert.equal(coverage_report(), "loose_first=true loose_done=Mastered strict_done=Mastered")
}

/// Noise names no model: a learner answering by a bit unrelated to the case
/// is suspected of reasoning as none of them does. Every model sits within
/// half a standard deviation of chance, which is what a suspicion measured
/// against the learner's own estimate is for.
function parity_report() -> string throws never {
    let sat = sit(parity, root.engine.default_knobs(), 10);
    `suspected=${sat.standing.suspected ?? "none"} why=${sat.standing.why} leading=${leading(sat.profile)}`
}

test "learner: answering by an unrelated bit is suspected of no model" {
    assert.equal(
        parity_report(),
        "suspected=none why=Budget leading=spelling=-1,covariant=-2,typescript=-4",
    )
}

test "learner: reasoning as TypeScript does is found out as TypeScript" {
    assert.equal(
        typescripter_report(),
        "suspected=typescript leading=typescript=53,covariant=28,spelling=-40 taught=true on_point=true both_kinds=true",
    )
}

test "learner: knowing only the axioms certifies no variance rule" {
    assert.equal(
        beginner_report(),
        "certified=false variance_mastered=0 some_axioms=true withheld_some=true",
    )
}

// ─── the points follow the bar ───────────────────────────────────────────────

/// Answering beats abstaining exactly above the bar.
function points_follow_the_bar() -> bool throws never {
    [0.5, 0.67, 0.75, 0.8].every((bar) -> {
        let p = root.engine.points(root.engine.Knobs { ...root.engine.default_knobs(), bar });
        let below = (bar - 0.05) * p.right * 1.0 + (1.05 - bar) * p.wrong * 1.0;
        let above = (bar + 0.05) * p.right * 1.0 + (0.95 - bar) * p.wrong * 1.0;
        below < p.unsure * 1.0 && above > p.unsure * 1.0
    })
}

test "points: answering beats abstaining exactly above the bar" {
    assert.is_true(points_follow_the_bar())
}

// ─── the profile is a value ──────────────────────────────────────────────────

function profile_round_trip() -> string throws never {
    let sat = sit(typescripter, root.engine.Knobs { ...root.engine.default_knobs(), budget: 12 }, 6);
    let text = baml.json.to_string(sat.profile) catch_all (e) {
        baml.json.SerializationError => {
            return "serialize threw";
        },
    };
    let back = baml.json.from_string<root.engine.Profile>(text) catch_all (e) {
        baml.json.ParseError => {
            return "parse threw";
        },
        baml.json.DecodeError => {
            return `decode threw on ${text}`;
        },
    };
    if (back == sat.profile) {
        "identical"
    } else {
        "differs"
    }
}

function sitting_is_deterministic() -> bool throws never {
    let knobs = root.engine.Knobs { ...root.engine.default_knobs(), budget: 15 };
    sit(typescripter, knobs, 7).served == sit(typescripter, knobs, 7).served
}

// ─── what a page keeps is enough ─────────────────────────────────────────────

/// The profile a sitting ended with is the profile its answers replay to:
/// nothing is believed about a learner that what they said does not determine.
function replay_matches() -> bool throws never {
    let knobs = root.engine.Knobs { ...root.engine.default_knobs(), budget: 12 };
    let sat = sit(typescripter, knobs, 6);
    // A self-mark is kept for the transcript and is not evidence: the page
    // records it after the profile has moved on, and replaying with it must
    // land in the same place.
    let marked = sat.taken.map((t) -> {
        root.Taken { ...t, given: root.Given { ...t.given, reasoning: "because", mark: "sound" } }
    });
    root.replay(knobs, sat.taken) == sat.profile && root.replay(knobs, marked) == sat.profile
}

function word_for(verdict_correct: bool?) -> string throws never {
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

/// One answer worked out all at once agrees with each part worked out on its
/// own: the profile with `observe_answer`, the mark and the compiler's words
/// with the key, and the points with the mark.
function answered_report() -> string throws never {
    let knobs = root.engine.default_knobs();
    let profile = root.fresh_profile();
    let next = root.next_prompt(profile, knobs, 8, 0)
        ?? return "nothing to ask";
    let item = next.prompt.item;
    let seed = next.prompt.seed;
    let put = root.put_at(knobs, item, seed);
    let answers = [
        root.Given { said: right_word(put), reasoning: "", mark: "" },
        root.Given { said: wrong_word(put), reasoning: "", mark: "" },
        root.Given { said: "unsure", reasoning: "", mark: "" },
    ];
    let answered = answers.map((given) -> {
        root.answer_case(profile, knobs, item, seed, given)
    });
    let marks = answered.map((a) -> {
        word_for(a.exchange.judgement.verdict_correct)
    });
    let points = answered.map((a) -> {
        a.points
    });
    let profiles = baml.iter.Range.new(0, answers.length()).every((i) -> {
        answered[i].profile == root.observe_answer(profile, knobs, item, seed, answers[i])
    });
    // Each shown program's verdict, read from the key, against the same
    // program's verdict as the compiler reports it: two routes to one fact.
    let compiler = answered.every((a) -> {
        a.revealed.programs.every((program) -> {
            program.compiles == program.reported.compiles
        })
    });
    `marks=${marks} points=${points} profiles=${profiles} compiler=${compiler}`
}

/// The JSON an adaptive sitting downloads is what `review` reads back, the
/// model's conclusions ride along, and the review says what it concluded.
function adaptive_round_trip() -> string throws never {
    // Every case that can be put as a choice is, so the round trip covers a
    // question of two programs as well as one of a single program: a
    // transcript has always held the second and never the first.
    let knobs = root.engine.Knobs { ...root.engine.default_knobs(), budget: 12, contrast: 1.0 };
    let sat = sit(typescripter, knobs, 9);
    let expected = root.adaptive_sitting(9, false, knobs, sat.taken);
    let text = root.adaptive_json(9, false, knobs, sat.taken);
    let back = baml.json.from_string<root.engine.Transcript>(text) catch_all (e) {
        baml.json.ParseError => {
            return "parsing threw";
        },
        baml.json.DecodeError => {
            return `decoding threw on ${text}`;
        },
    };
    let dir = scratch("adaptive");
    clean_up(dir);
    let reviewed = match (root.engine.save(expected, dir)) {
        let failed: root.engine.SaveFailed => `saving failed: ${failed.message}`,
        let saved: root.engine.Saved => root.review(saved.path),
    };
    clean_up(dir);
    let concluded = reviewed.includes("every case still behaves as recorded")
        && reviewed.includes("mastered")
        && reviewed.includes("ended Budget");
    // A sitting of six at the default rate holds at least one question of
    // two programs, so the round trip covers both kinds of question rather
    // than only the one a transcript has always held.
    let choices = expected.exchanges
        .filter((exchange) -> {
            exchange.question.posed is root.engine.Choice
        })
        .length();
    `same=${back == expected} exchanges=${expected.exchanges.length()} choices=${choices > 0} learned=${expected.learner?.profile == sat.profile} reviewed=${concluded}`
}

test "page: the profile is exactly what the answers replay to" {
    assert.is_true(replay_matches())
}

test "page: one answer worked out at once agrees with each part on its own" {
    assert.equal(
        answered_report(),
        `marks=["right", "wrong", "withheld"] points=[4, -12, 0] profiles=true compiler=true`,
    )
}

test "page: an adaptive sitting downloads, reads back, and reviews with what it concluded" {
    assert.equal(
        adaptive_round_trip(),
        "same=true exchanges=12 choices=true learned=true reviewed=true",
    )
}

test "profile: survives JSON" {
    assert.equal(profile_round_trip(), "identical")
}

test "profile: the same session seed serves the same cases" {
    assert.is_true(sitting_is_deterministic())
}

/// Every rule a verified item can conclude has an order; the unguarded-cycle
/// rule has none, since only the diverging item reaches it. The highest is
/// the double-contravariance fact's three: an axiom under two parameter
/// positions, which is as deep as the bank goes today.
function orders_report() -> string throws never {
    let orders = root.bank.rule_orders(4);
    let counted = [1, 2, 3].map((order) -> {
        orders
            .filter((o) -> {
                o.highest == order
            })
            .length()
    });
    let deepest = orders
        .filter((o) -> {
            o.highest == 3
        })
        .map((o) -> {
            o.rule
        });
    let never_alone = orders
        .filter((o) -> {
            o.lowest > 1
        })
        .length();
    `rules=${orders.length()} of ${root.bank.rules().length() - 1} by_order=${counted} deepest=${deepest} never_alone=${never_alone}`
}

test "orders: every servable rule has one, and only parameter contravariance reaches three" {
    assert.equal(
        orders_report(),
        `rules=25 of 26 by_order=[20, 4, 1] deepest=["variance/function-parameter-contravariant"] never_alone=5`,
    )
}

// ─── how often each answer comes up ──────────────────────────────────────────
//
// A sitting asks about three cases in five that compile, so a learner who
// always says so is right more often than not. It is not the selection's
// doing and the selection cannot mend it: most of the bank's items answer
// one way for every case they make (see the bank suite), so the mix of
// answers IS the mix of items, and forcing the answers even would force the
// items uneven — measured, insisting enough to move the share at all costs
// a scripted expert its certification and a beginner every axiom it had.
// The fix is a bank whose facts do not each answer one way: near misses, in
// §12 of the plan. This records where it stands until then.

/// Whether the question put at `t` asked for a verdict, and if so whether
/// its answer was that the program compiles.
function compiles_of(knobs: root.engine.Knobs, t: root.Taken) -> bool? throws never {
    match (root.put_at(knobs, t.item, t.seed).posed) {
        let case: root.engine.Case => {
            root.engine.verdict_kind(case.key) == root.engine.VerdictKind.Compiles
        },
        root.engine.Choice => null,
    }
}

/// Of the first sixteen questions of a sitting that asked for a verdict, how
/// many compiled and how many there were.
///
/// The sitting is stopped at the window, since running each to its own end
/// would cost several times as much for nothing.
function opening(learner: Learner, session: int, window: int) -> int[] throws never {
    let knobs = root.engine.Knobs { ...root.engine.default_knobs(), practice: true, budget: window };
    let verdicts = sit(learner, knobs, session).taken.filter_map((t) -> {
        compiles_of(knobs, t)
    });
    let compiled = verdicts
        .filter((v) -> {
            v
        })
        .length();
    [compiled, verdicts.length()]
}

/// `part` of `whole` as a whole percentage.
///
/// Through a function because the arithmetic reads locals bound from earlier
/// closure calls, and a method call on one of those reads the wrong slot
/// (README, compiler issue 9).
function percent_of(part: int, whole: int) -> int throws never {
    if (whole == 0) {
        return 0;
    }
    (part * 100.0 / (whole * 1.0)).iround() catch (e) {
        baml.errors.InvalidArgument => 0,
    }
}

/// How many of the opening questions compiled, pooled over three learners
/// and four sessions, in hundredths.
function leaning_report() -> string throws never {
    let learners: Learner[] = [expert, beginner, typescripter];
    let counted = learners.flat_map((learner) -> {
        [11, 12, 13, 14].map((session) -> {
            opening(learner, session, 16)
        })
    });
    let compiled = counted.reduce(
        (acc, c) -> {
            acc + c[0]
        },
        0,
    );
    let asked = counted.reduce(
        (acc, c) -> {
            acc + c[1]
        },
        0,
    );
    `compiled=${percent_of(compiled, asked)} of=${asked}`
}

test "sitting: three questions in five ask about a case that compiles" {
    assert.equal(leaning_report(), "compiled=62 of=169")
}
