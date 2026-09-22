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
    let given = root.Given { said: learner(put), reasoning: "", mark: "", marked_by: "" };
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

/// What certifying a perfect learner costs over `sessions`, and what the
/// first of them concluded about withholding and about the naive models.
///
/// Over twelve sittings, not one, and asked as two tests of six. What it
/// costs varies enormously with the draw — the spread runs from the low
/// eighties to the budget, and two of the twelve do not finish inside it —
/// so a single session measures the luck of that session, and the version of
/// this test that asked one certified by chance. Splitting the twelve is for
/// the clock rather than the measurement: driving them is the slowest thing
/// the suite does by a factor of two, and the per-test cap is what it would
/// meet first.
///
/// # Panics
///
/// - `baml.panics.Unreachable` if `sessions` is empty, which would be a
///   report about nothing.
function expert_report_over(sessions: int[]) -> string throws never {
    let knobs = root.engine.default_knobs();
    let sittings = sessions.map((session) -> {
        sit(expert, knobs, session)
    });
    let steps = sittings.map((sat) -> {
        `${sat.standing.why}:${sat.profile.step}`
    });
    let certified = steps
        .filter((step) -> {
            step.starts_with("Mastered")
        })
        .length();
    // The first sitting of the run, which the map above already sat: reading
    // `withheld` and `suspected` off a thirteenth sitting of the same session
    // cost an eighth of this test for a number it had already worked out.
    let first = sittings.at(0)
        ?? throw baml.panics.Unreachable { message: "a certification report over no sittings" };
    `certified=${certified} of ${sessions.length()} steps=${steps} withheld=${first.standing.withheld} suspected=${first.standing.suspected ?? "none"}`
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

/// A learner who holds back on everything shows, by holding back, that they
/// hold back when they do not know: the estimate of that has to follow them
/// down, or their silence would be read as honesty they have not shown.
function silent_report() -> string throws never {
    let sat = sit(silent, root.engine.default_knobs(), 2);
    let commits = sat.standing.commits_unsure ?? 1.0;
    `why=${sat.standing.why} mastered=${sat.standing.mastered.length()} committed=${sat.profile.committed} commits_unsure=${hundredths(commits)}`
}

/// What a case is worth to selection: a rule met once, the same rule met
/// twice, and two different rules met once each.
///
/// The update moves one offset per rule, with the whole of that rule's
/// gradient, so a rule a case reaches twice moves its offset twice as far and
/// the answer says FOUR times as much about it, not twice. That makes such a
/// case worth strictly more than two different rules met once, which move two
/// offsets one step each. Reading the information per occurrence instead
/// makes the two identical, and undervalues exactly the deep cases the ladder
/// exists to reach.
///
/// A fresh profile holds every rule at the same strength, so the comparison
/// turns on the arithmetic and not on which rules were drawn.
function information_report() -> string throws never {
    let profile = root.fresh_profile();
    let knobs = root.engine.default_knobs();
    let a = profile.skills.at(0)?.rule
        ?? return "no skills";
    let b = profile.skills.at(1)?.rule
        ?? return "one skill";
    let once = root.engine.information(profile, knobs, [a], 0);
    let twice = root.engine.information(profile, knobs, [a, a], 0);
    let apart = root.engine.information(profile, knobs, [a, b], 0);
    `once=${hundredths(once)} twice=${hundredths(twice)} apart=${hundredths(apart)} deeper=${twice > apart}`
}

test "information: a rule met twice is worth more than two rules met once" {
    assert.equal(information_report(), "once=34 twice=37 apart=28 deeper=true")
}

/// `x` as hundredths. Through a function: the arithmetic reads a local bound
/// from an earlier closure call (README, compiler issue 9).
function hundredths(x: float) -> int throws never {
    (x * 100.0).iround() catch (e) {
        baml.errors.InvalidArgument => 0,
    }
}

function strict_knobs(knobs: root.engine.Knobs) -> root.engine.Knobs throws never {
    root.engine.Knobs { ...knobs, strict: true, budget: 200 }
}

/// A strict sitting asks for every rule; a loose one stops on the overall
/// picture. The expert should be done sooner under loose than strict, and
/// both well within the budget.
function coverage_report() -> string throws never {
    let knobs = root.engine.default_knobs();
    let loose = sit(expert, knobs, 3);
    let strict = sit(expert, strict_knobs(knobs), 3);
    // How many rules the strict sitting left short of the bar, so the run
    // cannot pass by certifying an empty set.
    let uncertified = strict.profile.skills
        .filter((skill) -> {
            root.engine.logistic(root.engine.floor_of(strict.profile, strict_knobs(knobs), skill))
                < knobs.target
        })
        .length();
    `loose_first=${loose.profile.step <= strict.profile.step} loose_done=${loose.standing.why} loose_steps=${loose.profile.step} strict_done=${strict.standing.why} strict_steps=${strict.profile.step} uncertified=${uncertified} rules=${strict.profile.skills.length()}`
}

/// How hard the questions stay as a perfect learner improves: the predicted
/// chance they answer right, at every tenth question of a sitting, and the
/// number of rules the case turned on.
///
/// Selection aims at `knobs.aim`, so this makes the BANK'S CEILING visible.
/// While there are cases this learner might get wrong, the aim finds them and
/// the chance sits near the aim. Once every case the bank can draw is one they
/// would get right, the aim has nothing left to aim at, the chance runs away
/// to one, and the sitting spends its remaining questions asking things it
/// already knows the answer to. Three models that sat the quiz all said the
/// difficulty stopped climbing; this is that, as a number.
function escalation_report() -> string throws never {
    escalation_of(expert)
}

/// A learner who knows the axioms and withholds on anything deeper must NOT
/// be marched up the ladder: the case that would tell us most about a
/// perfect learner is one they cannot get a foothold on.
function beginner_escalation_report() -> string throws never {
    escalation_of(beginner)
}

function escalation_of(learner: Learner) -> string throws never {
    let knobs = root.engine.default_knobs();
    let sat = sit(learner, knobs, 3);
    let chances: int[] = [];
    let depths: int[] = [];
    let profile = sat.taken.reduce(
        (carried, t) -> {
            let put = root.put_at(knobs, t.item, t.seed);
            let rules = root.tracer_rules(put.posed);
            chances.push(hundredths(root.engine.predict(carried, knobs, rules, put.traps.length())));
            depths.push(rules.length());
            root.observe_answer(carried, knobs, t.item, t.seed, t.given)
        },
        root.fresh_profile(),
    );
    // Escalation is whether the questions get DEEPER as the learner improves,
    // so it is the mean depth of the opening twenty against the closing
    // twenty, not a reading every tenth question, which is too noisy to say
    // anything.
    let n = depths.length();
    let opening = mean_of(depths.slice(0, 20.min(n)));
    let closing = mean_of(depths.slice((n - 20).max(0), n));
    let spread = [1, 2, 3, 4, 5].map((d) -> {
        `${d}:${depths.filter((got) -> {
            got == d
        }).length()}`
    });
    `steps=${n} depth_first20=${opening} depth_last20=${closing} spread=${spread} chance_last20=${mean_of(chances.slice((n - 20).max(0), n))}`
}

/// How often the same item comes round in a perfect learner's sitting: the
/// five most-served items with their counts, and how many distinct items
/// the sitting drew on. Three models that sat the quiz each rated it five of
/// five for repetition and counted the same template six to ten times; this
/// is that, as a number, and `exposure` is what brought it down to the
/// handful the pin below records.
function repetition_report() -> string throws never {
    let sat = sit(expert, root.engine.default_knobs(), 3);
    let counts: map<string, int> = {};
    for (let item in sat.served) {
        counts.set(item, (counts.get(item) ?? 0) + 1);
    }
    let top = counts.keys()
        .sort_by_key((item) -> {
            0 - (counts.get(item) ?? 0)
        })
        .slice(0, 5)
        .map((item) -> {
            `${item.replace("flow/", "")}=${counts.get(item) ?? 0}`
        });
    `steps=${sat.served.length()} distinct=${counts.keys().length()} top=${top}`
}

test "learner: the same item comes round again and again" {
    assert.equal(
        repetition_report(),
        `steps=110 distinct=39 top=["param_contravariant=6", "array_element=6", "double_contravariance=6", "carried_2=5", "return_covariant=5"]`,
    )
}

/// The mean of `values`, in hundredths, or zero for none.
function mean_of(values: int[]) -> int throws never {
    if (values.length() == 0) {
        return 0;
    }
    let total = values.reduce(
        (acc, v) -> {
            acc + v
        },
        0,
    );
    percent_of(total, values.length())
}

test "learner: a learner of the axioms is not marched into depth" {
    assert.equal(
        beginner_escalation_report(),
        `steps=140 depth_first20=130 depth_last20=145 spread=["1:105", "2:23", "3:9", "4:3", "5:0"] chance_last20=6690`,
    )
}

test "learner: the questions stop getting harder once the bank runs out of depth" {
    assert.equal(
        escalation_report(),
        `steps=110 depth_first20=140 depth_last20=150 spread=["1:74", "2:18", "3:11", "4:4", "5:3"] chance_last20=8720`,
    )
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

/// A learner who knows the axioms and holds back on everything else is
/// certified on nothing — four right answers on a rule do not establish
/// that it is applied three times in four, let alone at a confidence shared
/// out over twenty-five rules — but the model must still be able to tell
/// what they know from what they do not. The test is therefore the
/// SEPARATION: their axioms stand above their variance rules, which they
/// never once committed to.
function mean_offset(sat: Sat, variance_rules: bool) -> int throws never {
    let offsets = sat.profile.skills
        .filter((skill) -> {
            skill.rule.starts_with("variance/") == variance_rules
        })
        .map((skill) -> {
            skill.offset
        });
    if (offsets.length() == 0) {
        return 0;
    }
    hundredths(
        offsets.reduce(
            (acc, offset) -> {
                acc + offset
            },
            0.0,
        )
            / (offsets.length() * 1.0),
    )
}

function beginner_report() -> string throws never {
    let sat = sit(beginner, root.engine.default_knobs(), 5);
    let variance = sat.standing.mastered.filter((rule) -> {
        rule.starts_with("variance/")
    });
    `certified=${sat.standing.why == root.engine.Why.Mastered} variance_mastered=${variance.length()} axioms_above_variance=${mean_offset(sat, false) > mean_offset(sat, true)} withheld_some=${sat.standing.withheld > 0.0}`
}

test "learner: certifying a perfect learner costs 91 to 130 questions over sessions 1 to 6, and one does not finish" {
    // The two that do not finish across the twelve are the measurement, not a
    // fluke to be tuned away: at this budget a learner who answers everything
    // right is still sometimes left uncertified, and three models that sat the
    // quiz said independently that it should have had their measure by about
    // question twenty-five. Raising the budget hides it; the estimator is
    // where it would be fixed.
    //
    // It was one in twelve until a wrap over an equivalence stopped bearing
    // the wrap's own rule (`root.algebra.transfer_decides`). That rule is now
    // evidenced only where the wrap decides something — over a strict pair —
    // so it takes about half as many questions' worth of evidence per
    // question, and two sittings run out of budget rather than one. The
    // evidence those cases used to give was never earned: the learner could
    // answer them without knowing the wrap was invariant.
    assert.equal(
        expert_report_over([1, 2, 3, 4, 5, 6]),
        `certified=5 of 6 steps=["Mastered:130", "Mastered:96", "Mastered:110", "Budget:140", "Mastered:91", "Mastered:97"] withheld=0.0 suspected=none`,
    )
}

test "learner: certifying a perfect learner costs 81 to 133 questions over sessions 7 to 12, and one does not finish" {
    assert.equal(
        expert_report_over([7, 8, 9, 10, 11, 12]),
        `certified=5 of 6 steps=["Mastered:118", "Mastered:81", "Mastered:99", "Budget:140", "Mastered:110", "Mastered:133"] withheld=0.0 suspected=none`,
    )
}

test "learner: practice asks its count and stops nowhere else" {
    assert.equal(practice_report(), "certified=Mastered practised=Budget asked_all=true")
}

test "learner: a learner who never commits is never certified" {
    assert.equal(silent_report(), "why=Stalled mastered=0 committed=0 commits_unsure=2")
}

test "learner: strict coverage costs no fewer questions than an overall picture" {
    assert.equal(
        coverage_report(),
        "loose_first=true loose_done=Mastered loose_steps=110 strict_done=Mastered strict_steps=110 uncertified=0 rules=31",
    )
}

/// Noise names no model: a learner answering by a bit unrelated to the case
/// is suspected of reasoning as none of them does, and the model reads their
/// never holding back for what it is — they commit to an answer they do not
/// have almost every time, which is what makes their right answers worth
/// little.
function parity_report() -> string throws never {
    let sat = sit(parity, root.engine.default_knobs(), 10);
    let commits = sat.standing.commits_unsure ?? 0.0;
    `suspected=${sat.standing.suspected ?? "none"} why=${sat.standing.why} commits_unsure=${hundredths(commits)} leading=${leading(sat.profile)}`
}

test "learner: answering by an unrelated bit is suspected of no model" {
    assert.equal(
        parity_report(),
        "suspected=none why=Stalled commits_unsure=95 leading=spelling=14,shape=12,bounds=10,covariant=5,typescript=4,structural=0",
    )
}

test "learner: reasoning as TypeScript does is found out as TypeScript" {
    assert.equal(
        typescripter_report(),
        "suspected=typescript leading=typescript=61,bounds=25,covariant=22,structural=0,shape=-16,spelling=-23 taught=true on_point=true both_kinds=true",
    )
}

test "learner: knowing only the axioms tells the axioms apart from the rest" {
    assert.equal(
        beginner_report(),
        "certified=false variance_mastered=0 axioms_above_variance=true withheld_some=true",
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
        root.Given { said: right_word(put), reasoning: "", mark: "", marked_by: "" },
        root.Given { said: wrong_word(put), reasoning: "", mark: "", marked_by: "" },
        root.Given { said: "unsure", reasoning: "", mark: "", marked_by: "" },
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
    let expected = root.adaptive_sitting(9, false, knobs, sat.taken, "test-build");
    let text = root.adaptive_json(9, false, knobs, sat.taken, "test-build");
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

/// Every numeric knob against the domain table, found by reflection rather
/// than by a list here, so a knob added later without a domain fails this
/// instead of reaching a learner's save unchecked. Booleans have no domain.
function knob_domains_report() -> string throws never {
    let declared = reflect.Type.of<root.engine.Knobs>().as_class()
        ?? return "Knobs is not a class";
    let boolean = reflect.Type.of<bool>();
    let numeric = declared.fields()
        .filter((field) -> {
            field.type != boolean
        })
        .map((field) -> {
            field.name
        });
    let stated = root.engine.knob_domains(root.engine.default_knobs()).map((domain) -> {
        domain.name
    });
    let unstated = numeric.filter((name) -> {
        !stated.includes(name)
    });
    let extra = stated.filter((name) -> {
        !numeric.includes(name)
    });
    `unstated=${unstated} extra=${extra} defaults=${root.engine.knobs_fault(root.engine.default_knobs()) ?? "legal"}`
}

test "knobs: every numeric knob has a domain, and the defaults sit inside them" {
    assert.equal(knob_domains_report(), `unstated=[] extra=[] defaults=legal`)
}

/// What a save that was edited by hand, or written by a build with other
/// bounds, gets told. A probability out of range answers rather than
/// panicking, which is the case worth pinning; an infinity parses from JSON
/// (`1e999`) and passes every comparison a bound is written as unless
/// finiteness is asked for; and a budget of nothing ends a sitting before it
/// starts.
function hostile_knobs_report() -> string throws never {
    let base = root.engine.default_knobs();
    let slipping = root.engine.knobs_fault(root.engine.Knobs { ...base, slip: 2.0 }) ?? "legal";
    let endless = root.engine.knobs_fault(root.engine.Knobs { ...base, drift: float.inf() }) ?? "legal";
    let empty = root.engine.knobs_fault(root.engine.Knobs { ...base, budget: 0 }) ?? "legal";
    let certain = root.engine.knobs_fault(root.engine.Knobs { ...base, target: 1.0 }) ?? "legal";
    `slip: ${slipping}\ndrift: ${endless}\nbudget: ${empty}\ntarget: ${certain}`
}

test "knobs: a save with a knob out of its domain is refused, and says which" {
    assert.equal(
        hostile_knobs_report(),
        "slip: slip is 2.0, and it has to be between zero and one\ndrift: drift is Infinity, and it has to be a finite number no less than zero\nbudget: budget is 0, and it has to be a whole number no less than 1\ntarget: target is 1.0, and it has to be strictly between zero and one",
    )
}

test "profile: survives JSON" {
    assert.equal(profile_round_trip(), "identical")
}

test "profile: the same session seed serves the same cases" {
    assert.is_true(sitting_is_deterministic())
}

/// Every rule a verified item can conclude has an order. The two site rules
/// are not among them — a site is the frame a case stands in rather than
/// something to master — and neither is the unguarded-cycle rule, since only
/// the diverging item reaches it, which is the one the count below is short
/// of. The three carrying variance rules reach five: an axiom under four
/// wraps, which is as deep as the bank goes today.
function orders_report() -> string throws never {
    let orders = root.bank.rule_orders(4);
    let counted = [1, 2, 3, 4, 5].map((order) -> {
        orders
            .filter((o) -> {
                o.highest == order
            })
            .length()
    });
    let deepest = orders
        .filter((o) -> {
            o.highest >= 4
        })
        .map((o) -> {
            o.rule
        });
    let never_alone = orders
        .filter((o) -> {
            o.lowest > 1
        })
        .length();
    let servable = root.bank.rules()
        .filter((rule) -> {
            !root.bank.is_site_rule(rule.name)
        })
        .length();
    `rules=${orders.length()} of ${servable} by_order=${counted} deepest=${deepest} never_alone=${never_alone}`
}

test "orders: every servable rule has one, and only the carrying variance rules reach four or more" {
    assert.equal(
        orders_report(),
        `rules=31 of 32 by_order=[27, 1, 0, 1, 2] deepest=["variance/function-parameter-contravariant", "variance/function-return-covariant", "variance/function-error-covariant"] never_alone=4`,
    )
}

// ─── what a sitting's questions give away ────────────────────────────────────
//
// The bank is held to saying nothing with its surface (the tells suite), and
// selection could still undo that, because it prefers some cases to others.
// It did so twice over until it was kept from seeing the coins that decide a
// case's answer: it served whichever direction of a draw fooled a naive
// model — function types rejected ten times in eleven — and whichever fact
// of a shape held the rule the tracer wanted evidence on, which for an
// equivalence is always a case that compiles — unions accepted seven times
// in ten. It reads a draw's castings now (`root.engine.Candidate`), and this
// holds the questions a learner is actually asked to the bar.

/// The question put at `t`, as its surface and its answer, when it asked for
/// a verdict. The surface here is the words in the source: a served question
/// is a case, which carries its program and not the types it was built from.
function served_surface(knobs: root.engine.Knobs, t: root.Taken) -> Surfaced? throws never {
    match (root.put_at(knobs, t.item, t.seed).posed) {
        let case: root.engine.Case => {
            Surfaced {
                fact: t.item,
                compiles: root.engine.verdict_kind(case.key) == root.engine.VerdictKind.Compiles,
                features: text_features(source_of(case)),
            }
        },
        root.engine.Choice => null,
    }
}

/// The verdict questions among the first `window` of a sitting.
///
/// The sitting is stopped at the window, since running each to its own end
/// would cost several times as much for nothing.
function opening(learner: Learner, session: int, window: int) -> Surfaced[] throws never {
    let knobs = root.engine.Knobs { ...root.engine.default_knobs(), practice: true, budget: window };
    sit(learner, knobs, session).taken.filter_map((t) -> {
        served_surface(knobs, t)
    })
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

/// How often the questions three learners are asked compile, over four
/// sessions each, and every word of their source that leans more than
/// fifteen points from that over at least forty questions, with the items
/// that carry it.
///
/// Each learner sits sessions of their own. A session's seed is what draws
/// the candidates at each step, so learners sharing one are offered the same
/// cases and mostly take them: twelve sittings over four sessions are nearer
/// four samples than twelve, and one case asked three times reads as a lean.
function served_leaning_report() -> string throws never {
    let sittings: Sitting[] = [
        Sitting { learner: expert, sessions: [11, 12, 13, 14] },
        Sitting { learner: beginner, sessions: [21, 22, 23, 24] },
        Sitting { learner: typescripter, sessions: [31, 32, 33, 34] },
    ];
    let asked = sittings.flat_map((sitting) -> {
        sitting.sessions.flat_map((session) -> {
            opening(sitting.learner, session, 24)
        })
    });
    // Pass `true` to see which items carry a lean and how often each was
    // asked, which is where a lean says what it is made of.
    leaning_of(asked, 40, 15, false)
}

/// A learner and the sessions they sit.
class Sitting {
    learner: Learner,
    sessions: int[],
}

test "sitting: nothing in the questions a learner is asked leans more than fifteen points" {
    assert.equal(served_leaning_report(), `compiles=48% of 237 leaning=[]`)
}
