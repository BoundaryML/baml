// Conformance suite for the learner model: scripted learners sit the quiz
// through the same surface a page uses, and what the model concludes about
// them must match who they are. An expert is certified well under the budget,
// a learner who never commits is never certified, a learner who reasons as
// one naive model does is found out as that model, the points follow the bar,
// and a profile survives the trip to JSON.

// ─── scripted learners ───────────────────────────────────────────────────────

/// A learner is a rule for answering a case whose key it can see: `null` to
/// withhold, otherwise whether it says the case compiles.
type Learner = (root.engine.Candidate) -> bool? throws never;

function key_compiles(candidate: root.engine.Candidate) -> bool throws never {
    root.engine.verdict_kind(candidate.case.key) == root.engine.VerdictKind.Compiles
}

function expert(candidate: root.engine.Candidate) -> bool? throws never {
    key_compiles(candidate)
}

function silent(candidate: root.engine.Candidate) -> bool? throws never {
    null
}

/// Answers exactly as the TypeScript model would: right unless the case is a
/// trap for it.
function typescripter(candidate: root.engine.Candidate) -> bool? throws never {
    if (candidate.traps.includes("typescript")) {
        !key_compiles(candidate)
    } else {
        key_compiles(candidate)
    }
}

/// Knows the axioms cold and nothing about variance: right on one-rule cases,
/// withholds on the rest.
function beginner(candidate: root.engine.Candidate) -> bool? throws never {
    let flow = root.bank.flow_requires_subtype().name;
    let own = candidate.case.trace.filter((claim) -> {
        claim.rule.name != flow
    });
    if (own.length() <= 1) {
        key_compiles(candidate)
    } else {
        null
    }
}

/// One sitting: the learner answers whatever the model serves until the
/// model says the sitting is over.
class Sat {
    profile: root.engine.Profile,
    standing: root.engine.Standing,
    served: string[],
}

function sit(learner: Learner, knobs: root.engine.Knobs, session: int) -> Sat throws never {
    let start = root.fresh_profile();
    sit_from(learner, knobs, session, start, 0, [])
}

function sit_from(
    learner: Learner,
    knobs: root.engine.Knobs,
    session: int,
    profile: root.engine.Profile,
    step: int,
    served: string[],
) -> Sat throws never {
    let standing = root.engine.standing(profile, knobs);
    if (standing.why != root.engine.Why.Continue) {
        return Sat { profile, standing, served };
    }
    let next = root.next_prompt(profile, knobs, session, step)
        ?? return Sat { profile, standing, served };
    let candidate = root.candidate_of(next.prompt.item, next.prompt.seed)
        ?? return Sat { profile, standing, served };
    let said = match (learner(candidate)) {
        let says_compiles: bool => {
            if (says_compiles) {
                "compiles"
            } else {
                "rejected"
            }
        },
        null => "unsure",
    };
    let given = root.Given { said, reasoning: "", mark: "" };
    let after = root.observe_answer(profile, knobs, next.prompt.item, next.prompt.seed, given);
    sit_from(learner, knobs, session, after, step + 1, served.concat([next.prompt.item]))
}

// ─── what the model concludes ────────────────────────────────────────────────

function expert_report() -> string throws never {
    let knobs = root.engine.default_knobs();
    let sat = sit(expert, knobs, 1);
    let calibration = sat.standing.calibration ?? 0.0;
    `why=${sat.standing.why} within_budget=${sat.profile.step < knobs.budget} withheld=${sat.standing.withheld} calibrated=${calibration >= 0.95}`
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

function suspected(profile: root.engine.Profile) -> string throws never {
    profile.suspicions
        .sort_by_key((s) -> {
            0.0 - s.score
        })
        .at(0)
        ?.model
        ?? "none"
}

function typescripter_report() -> string throws never {
    let sat = sit(typescripter, root.engine.default_knobs(), 4);
    let by_model = sat.profile.suspicions.sort_by_key((s) -> {
        0.0 - s.score
    });
    let top = by_model.at(0)?.score ?? 0.0;
    let runner_up = by_model.at(1)?.score ?? 0.0;
    `suspected=${suspected(sat.profile)} clear=${top >= 2.0 * runner_up && top >= 3.0}`
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
    assert.equal(expert_report(), "why=Mastered within_budget=true withheld=0.0 calibrated=true")
}

test "learner: a learner who never commits is never certified" {
    assert.equal(silent_report(), "why=Stalled mastered=0 committed=0")
}

test "learner: strict coverage costs more questions than an overall picture" {
    assert.equal(coverage_report(), "loose_first=true loose_done=Mastered strict_done=Mastered")
}

test "learner: reasoning as TypeScript does is found out as TypeScript" {
    assert.equal(typescripter_report(), "suspected=typescript clear=true")
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
