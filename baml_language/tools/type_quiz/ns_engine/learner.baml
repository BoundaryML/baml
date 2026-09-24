// What we believe about a learner, and how a sitting should go for them.
//
// THE MODEL. A question is answered right when the learner applies every rule
// its case turns on and gets away with it. Writing `s_r` for how strongly
// they hold rule `r`, in log-odds, and `q` for the chance they work the whole
// case out:
//
//     s_r = ability + offset(r)
//     q   = Π σ(s_r) · (1 − trap_slip)^traps
//
// and then, for the three things that can happen:
//
//     P(right)    = q(1 − slip) + (1 − q)·commits/2
//     P(wrong)    = q·slip      + (1 − q)·commits/2
//     P(withheld) = (1 − q)(1 − commits)
//
// `slip` is erring while knowing; `commits` is committing while not knowing,
// which is estimated from the learner rather than assumed; the half is the
// coin a committed guess comes down to, the same for either kind of question
// this quiz asks. This is the conjunctive (DINA) model of skills, with the
// abstention as a third outcome rather than a discounted wrong answer.
//
// WHY THE PRODUCT. A case that turns on three rules is harder than one that
// turns on one, with no hand-set penalty for depth. The gradient of `q` in a
// rule the learner already holds is nearly zero, so an answer moves such a
// rule barely at all and narrows it barely at all: blame is DERIVED from the
// model instead of apportioned by a rule of thumb, and joint evidence stops
// counting as though it were independent evidence about each rule.
//
// THE UPDATE is the Gaussian (Laplace) posterior for one observation, which
// is what Elo and Glicko are when done properly: the mean moves by the score
// of the log-likelihood and the variance falls by the FISHER INFORMATION the
// outcome actually carries. An answer the model already expected narrows
// nothing; an answer at even odds narrows a lot. Between observations every
// variance grows a little, which is what keeps the filter answering to
// recent evidence and makes a rule left alone need asking about again.
//
// A profile is a value with no enum in it, because it crosses into BAML from
// the page and an enum argument arrives as a bare string (see main.baml).
// Every tunable is a field of `Knobs`, so nothing here is a constant that a
// learner would have to be re-served to change.

/// A rule and the orders it can conclude a case at: `lowest` is where it is
/// first met (one for a rule that can stand alone, two for one that always
/// wraps another), `highest` how deep the bank ever takes it.
class RuleOrder {
    rule: string,
    lowest: int,
    highest: int,
}

/// What we believe about one rule, relative to the learner's ability.
class Skill {
    rule: string,
    /// The lowest and highest orders this rule appears at in the bank.
    lowest: int,
    order: int,
    /// Deviation from the global ability, in log-odds. Its prior is centred
    /// on zero, which is the pooling: a rule with little evidence of its own
    /// stays near the global estimate.
    offset: float,
    /// The VARIANCE of `offset`, not its spread. Falls by the information an
    /// answer carries about this rule and grows a little with every
    /// observation, so a rule left alone becomes uncertain again.
    variance: float,
    /// The floor when this sitting last made progress, which is what the next
    /// answer's gain is measured against. Null until the first answer: there
    /// is nothing to compare a first floor with.
    marked: float?,
    /// The highest order answered correctly; zero when never.
    highest_right: int,
    /// Consecutive answers that were wrong or withheld. Drives teaching.
    wrong_streak: int,
    seen: int,
    /// The profile's step when this rule was last in a served case; zero when
    /// never.
    last_step: int,
}

/// How much a learner's errors look like one naive model's predictions.
///
/// The score is measured against chance: on each case that traps the model,
/// it moves by how much worse the learner did than their own estimate
/// predicted, so a learner who is right as often as expected drifts nowhere,
/// however many traps they meet, while one who answers as the model would
/// falls short every time. `variance` is what the score's spread would be by
/// chance, so `score / sqrt(variance)` reads as a z-score.
class Suspicion {
    model: string,
    score: float,
    variance: float,
}

/// How far a suspicion stands from chance, in standard deviations; zero
/// before any evidence.
function suspicion_z(suspicion: Suspicion) -> float throws never {
    if (suspicion.variance <= 0.0) {
        0.0
    } else {
        suspicion.score / baml.Float.sqrt(suspicion.variance)
    }
}

class Profile {
    /// Global ability, in log-odds.
    ability: float,
    /// The VARIANCE of `ability`, not its spread.
    variance: float,
    /// Observations so far, withheld answers included.
    step: int,
    /// Answers that gave a verdict.
    committed: int,
    /// Committed answers that were right.
    right: int,
    /// Answers the learner held back on.
    held_back: int,
    /// The model's own count of how many questions the learner did not know
    /// the answer to: the sum of `1 − q` over the questions put. Held back
    /// answers divided by this estimates how often they hold back when they
    /// do not know, which is the one thing that makes a committed answer
    /// worth anything.
    unknown_mass: float,
    skills: Skill[],
    suspicions: Suspicion[],
    /// How many times each item has been served this sitting, by id.
    served: map<string, int>,
    /// Consecutive observations that taught us nothing: no rule's estimate
    /// rose by more than a hair, and no rule was met for the first time by
    /// an answer the learner committed to.
    ///
    /// The estimate rather than the floor, because shrinking uncertainty
    /// raises a floor even on bad news. First sight of a rule counts,
    /// because reaching a rule nobody had been asked about is progress even
    /// for a learner who answers everything right and so has no estimate
    /// left to raise. Only on a committed answer, because a learner who
    /// withholds on everything would otherwise be credited with progress for
    /// meeting rules they said nothing about, and never be called stuck.
    quiet: int,
}

/// Every tunable of the model, in one place.
class Knobs {
    /// The confidence below which a learner should say they are not sure.
    /// Sets the guess rate a committed answer is read with, and the points.
    bar: float,
    /// How reliably a rule must be applied for us to call it mastered.
    target: float,
    /// The chance we accept of being wrong about ANY rule. Not a number of
    /// standard deviations: the bound is taken at `confidence / rules` per
    /// rule, so that testing twenty-five of them at once is still the claim
    /// it reads as. See `z_for`.
    confidence: float,
    /// Coverage: every rule mastered on its own (`true`), or the general
    /// level confident with no rule confidently lagging it by more than
    /// `band` in log-odds. `band` has to clear what the pooling prior alone
    /// allows — a rule nothing is known about sits `rule_z` times the prior
    /// spread below zero — or no sitting could ever end.
    strict: bool,
    band: float,
    /// The most questions a sitting may ask.
    ///
    /// An item that serves an equivalence half the time and its near miss the
    /// other half evidences the equivalence's rule on half its draws, so
    /// certifying one costs about twice the questions it would if selection
    /// could choose the equivalence — which is the choice it is kept from, so
    /// the kind of question cannot give its answer away. A wrap over an
    /// equivalence evidences only the rule it carries, for the same reason:
    /// the wrap itself decides nothing there.
    ///
    /// Measured over twelve sittings: 81 to 133 questions, and two of the
    /// twelve are not certified inside this budget. Raising it would hide
    /// that rather than mend it — the estimator is where it would be mended.
    budget: int,
    /// Stop after this many observations that moved no rule's floor, once
    /// at least `minimum` have been made: a sitting is given time to reach
    /// every rule before it can be called stuck.
    stall_after: int,
    minimum: int,
    /// The success probability selection aims a case at, when `inform` is
    /// zero.
    aim: float,
    /// How strongly selection follows expected information. Zero aims at a
    /// success rate instead (`aim`); above zero the draw is in proportion to
    /// the information a case is expected to carry, raised to this power,
    /// and `aim` is not used. The two are different answers to what a
    /// question is for: the aim is the success rate at which people are said
    /// to learn best, and information is what measuring them asks for.
    inform: float,
    /// Penalty per time an item has already been served this sitting. A rule
    /// may come round as often as the estimate wants it; the same ITEM coming
    /// round is the same template again, which is what a learner calls
    /// repetitive. Zero turns it off.
    exposure: float,
    /// Rules seen within this many steps are avoided.
    rest: int,
    /// A mastered rule is rechecked after this many steps.
    recheck: int,
    /// How often a case that has a foil is put as a choice between the two
    /// programs rather than as a verdict on the one.
    ///
    /// Only a strict relation has a second program to offer, which is a
    /// third of the bank's cases, so the share of a whole sitting spent on
    /// choices is about half of this. Half of what can be a choice is the
    /// default because a choice is the one question whose two programs have
    /// the same size, shape and rule count: nothing on their surface
    /// correlates with the answer, which is not true of a program judged
    /// alone. How much better they teach is a question for real transcripts
    /// rather than for a number chosen here.
    contrast: float,
    /// Stop at the budget and nowhere else: a fixed number of cases,
    /// adapting all the way. Practice, as against certification.
    practice: bool,
    /// The chance of erring while knowing. Without it a single wrong answer
    /// from a learner the model is sure about would be impossible rather
    /// than unlucky, and the estimate would lurch.
    slip: float,
    /// The chance one naive model's trap catches a learner who otherwise
    /// works the case out. Each trap on a case gets its own chance.
    trap_slip: float,
    /// How much variance every estimate gains per observation. What a
    /// learner knows drifts, and a filter that never widens stops answering
    /// to new evidence; it also makes a rule left alone worth asking about
    /// again.
    drift: float,
    /// The z-score a naive model's suspicion must reach to be named as the
    /// way the learner reasons, and how far, also in standard deviations,
    /// it must stand clear of the next model: the models overlap, so a
    /// learner who reasons as one does also looks a little like another.
    suspect: float,
    clear: float,
}

function default_knobs() -> Knobs throws never {
    Knobs {
        bar: 0.75,
        target: 0.75,
        confidence: 0.1,
        strict: false,
        band: 1.5,
        budget: 140,
        stall_after: 20,
        minimum: 40,
        aim: 0.8,
        inform: 1.0,
        exposure: 1.5,
        rest: 3,
        recheck: 15,
        contrast: 0.5,
        practice: false,
        slip: 0.08,
        trap_slip: 0.15,
        drift: 0.002,
        suspect: 3.0,
        clear: 1.0,
    }
}

/// What we believe before a learner has answered anything: they are as
/// likely to know a rule as not, and we could be wrong about that by a wide
/// margin. The rule prior is narrower than the global one because a rule is
/// only ever a deviation from the ability, which is what pools them.
function prior_variance() -> float throws never {
    2.25
}

function prior_rule_variance() -> float throws never {
    0.25
}

/// A profile that has seen nothing.
function fresh(rules: RuleOrder[], models: string[]) -> Profile throws never {
    Profile {
        ability: 0.0,
        variance: prior_variance(),
        step: 0,
        committed: 0,
        right: 0,
        held_back: 0,
        unknown_mass: 0.0,
        skills: rules.map((r) -> {
            Skill {
                rule: r.rule,
                lowest: r.lowest,
                order: r.highest,
                offset: 0.0,
                variance: prior_rule_variance(),
                marked: null,
                highest_right: 0,
                wrong_streak: 0,
                seen: 0,
                last_step: 0,
            }
        }),
        suspicions: models.map((model) -> {
            Suspicion { model, score: 0.0, variance: 0.0 }
        }),
        served: {},
        quiet: 0,
    }
}

// ─── the arithmetic ──────────────────────────────────────────────────────────

function logistic(x: float) -> float throws never {
    1.0 / (1.0 + baml.Float.exp(0.0 - x))
}

/// `numerator / denominator`, with the denominator held off zero. Every
/// division below is by a probability the model itself produced, and one of
/// them can come arbitrarily close to zero.
function ratio(numerator: float, denominator: float) -> float throws never {
    numerator / denominator.max(0.000001)
}

function skill_of(profile: Profile, rule: string) -> Skill? throws never {
    profile.skills.find((skill) -> {
        skill.rule == rule
    })
}

/// How strongly the learner holds `rule`, in log-odds.
function strength(profile: Profile, rule: string) -> float throws never {
    profile.ability + (skill_of(profile, rule)?.offset ?? 0.0)
}

/// The chance the learner works a case of these rules out: every rule
/// applied correctly, and no trap on it caught them.
///
/// A rule the case reaches twice must be applied twice, which the product
/// counts for nothing extra in code and everything in meaning.
function knowing(
    profile: Profile,
    knobs: Knobs,
    rules: string[],
    traps: int,
) -> float throws never {
    let held = rules.reduce(
        (acc, rule) -> {
            acc * logistic(strength(profile, rule))
        },
        1.0,
    );
    held * baml.Float.pow(1.0 - knobs.trap_slip, traps * 1.0)
}

/// How often the learner commits to an answer they do not have.
///
/// The bar asks them to hold back below it, so the prior is that they commit
/// `1 - bar` of the time, worth a few observations against what the sitting
/// has shown. ESTIMATED RATHER THAN ASSUMED, because the assumption fails in
/// the unsafe direction: a learner who never says "not sure" is guessing at
/// even odds, and reading their right answers as though they were not would
/// certify them on a fraction of the evidence they have given.
function commits(profile: Profile, knobs: Knobs) -> float throws never {
    let prior = 4.0;
    let holds = ratio(profile.held_back * 1.0 + prior * knobs.bar, profile.unknown_mass + prior);
    (1.0 - holds).max(0.02).min(0.98)
}

/// What the three outcomes are worth, or how each moves with `q`.
class Chances {
    right: float,
    wrong: float,
    withheld: float,
}

function chances(q: float, knobs: Knobs, commits: float) -> Chances throws never {
    Chances {
        right: q * (1.0 - knobs.slip) + (1.0 - q) * commits / 2.0,
        wrong: q * knobs.slip + (1.0 - q) * commits / 2.0,
        withheld: (1.0 - q) * (1.0 - commits),
    }
}

/// The derivative of each chance in `q`, which is what carries an answer
/// back to the rules it turned on.
function slopes(knobs: Knobs, commits: float) -> Chances throws never {
    Chances {
        right: 1.0 - knobs.slip - commits / 2.0,
        wrong: knobs.slip - commits / 2.0,
        withheld: 0.0 - (1.0 - commits),
    }
}

/// Which of the three an answer was.
function taken(of: Chances, verdict: bool?) -> float throws never {
    match (verdict) {
        let right: bool => {
            if (right) {
                of.right
            } else {
                of.wrong
            }
        },
        null => of.withheld,
    }
}

/// What one answer on a case of these rules is expected to tell us about
/// the learner, before it is seen: the trace of the Fisher information over
/// the ability and every rule's offset.
///
/// This is what a question is WORTH. A case the learner would surely get
/// right carries nothing, and so does one they would surely miss; a case on
/// rules they half hold carries the most, and a case on several such rules
/// carries more than a case on one. For a learner who holds every rule, no
/// single-rule case is worth anything and only depth is — which makes
/// escalation a consequence of asking worthwhile questions rather than a
/// policy laid on top.
function information(
    profile: Profile,
    knobs: Knobs,
    rules: string[],
    traps: int,
) -> float throws never {
    let q = knowing(profile, knobs, rules, traps);
    let commit_rate = commits(profile, knobs);
    let chance = chances(q, knobs, commit_rate);
    let slope = slopes(knobs, commit_rate);
    // What an outcome says about `q`, summed over the three outcomes.
    let about_q = ratio(slope.right * slope.right, chance.right)
        + ratio(slope.wrong * slope.wrong, chance.wrong)
        + ratio(slope.withheld * slope.withheld, chance.withheld);
    // How `q` moves with the ability and with each rule's own offset: the
    // same gradients the update uses, so what selection values is exactly
    // what the answer will move.
    let doubts = rules.map((rule) -> {
        1.0 - logistic(strength(profile, rule))
    });
    let total_doubt = doubts.reduce(
        (acc, doubt) -> {
            acc + doubt
        },
        0.0,
    );
    let by_ability = q * total_doubt * q * total_doubt;
    // Per rule rather than per occurrence, over the same skills the update
    // walks. A rule the case reaches twice moves its one offset with twice
    // the gradient, so it carries four times the information and not twice:
    // summing the occurrences instead understates exactly the deep cases,
    // which are the ones worth reaching for.
    let by_rules = profile.skills.reduce(
        (acc, skill) -> {
            let gradient = q * times_in(rules, skill.rule) * (1.0 - logistic(strength(profile, skill.rule)));
            acc + gradient * gradient
        },
        0.0,
    );
    about_q * (by_ability + by_rules)
}

/// The chance of a right answer: what selection aims at, and what a
/// suspicion's residual is measured against.
function predict(
    profile: Profile,
    knobs: Knobs,
    rules: string[],
    traps: int,
) -> float throws never {
    chances(knowing(profile, knobs, rules, traps), knobs, commits(profile, knobs)).right
}

// ─── how sure we are ─────────────────────────────────────────────────────────

/// The number of standard deviations that leaves `tail` of a normal above
/// it, by Acklam's rational approximation to the inverse normal.
///
/// # Panics
///
/// - `baml.panics.Unreachable` if `tail` is not strictly between zero and
///   one, where no such number exists.
function z_for(tail: float) -> float throws never {
    if (tail <= 0.0 || tail >= 1.0) {
        throw baml.panics.Unreachable { message: `no bound leaves ${tail} of a normal above it` }
    }
    0.0 - inverse_normal(tail)
}

/// The value below which `p` of a standard normal lies.
function inverse_normal(p: float) -> float throws never {
    let low = 0.02425;
    if (p < low) {
        return lower_tail(p);
    }
    if (p > 1.0 - low) {
        return 0.0 - lower_tail(1.0 - p);
    }
    let q = p - 0.5;
    let r = q * q;
    let numerator = (
        (
            (((0.0 - 39.69683028665376) * r + 220.9460984245205) * r - 275.9285104469687) * r
                + 138.357751867269
        )
            * r
            - 30.66479806614716
    )
        * r
        + 2.506628277459239;
    let denominator = (
        (
            (((0.0 - 54.47609879822406) * r + 161.5858368580409) * r - 155.6989798598866) * r
                + 66.80131188771972
        )
            * r
            - 13.28068155288572
    )
        * r
        + 1.0;
    numerator * q / denominator
}

/// The approximation's own tail branch, for `p` below its central range.
function lower_tail(p: float) -> float throws never {
    let q = baml.Float.sqrt(0.0 - 2.0 * baml.Float.ln(p));
    let numerator = (
        (
            (((0.0 - 0.007784894002430293) * q - 0.3223964580411365) * q - 2.400758277161838) * q
                - 2.549732539343734
        )
            * q
            + 4.374664141464968
    )
        * q
        + 2.938163982698783;
    let denominator = (
        ((0.007784695709041462 * q + 0.3224671290700398) * q + 2.445134137142996) * q
            + 3.754408661907416
    )
        * q
        + 1.0;
    numerator / denominator
}

/// The bound each rule is judged at. Twenty-five rules tested at once is
/// twenty-five chances to be wrong, so the family-wise chance is divided
/// among them: without that, `confidence` of one in ten would expect two and
/// a half rules to sit below their own floor and still read as certain.
function rule_z(profile: Profile, knobs: Knobs) -> float throws never {
    let rules = profile.skills.length().max(1) * 1.0;
    z_for(knobs.confidence / rules)
}

/// One knob's value against the values it may take.
class KnobDomain {
    name: string,
    /// The value, as written in a reason.
    value: string,
    holds: bool,
    /// The domain in words.
    domain: string,
}

/// Strictly between zero and one: a probability whose endpoints stop meaning
/// what the knob says.
function open_unit(name: string, value: float) -> KnobDomain throws never {
    KnobDomain {
        name,
        value: `${value}`,
        holds: value.is_finite() && value > 0.0 && value < 1.0,
        domain: "strictly between zero and one",
    }
}

function closed_unit(name: string, value: float) -> KnobDomain throws never {
    KnobDomain {
        name,
        value: `${value}`,
        holds: value.is_finite() && value >= 0.0 && value <= 1.0,
        domain: "between zero and one",
    }
}

function positive(name: string, value: float) -> KnobDomain throws never {
    KnobDomain {
        name,
        value: `${value}`,
        holds: value.is_finite() && value > 0.0,
        domain: "a finite number above zero",
    }
}

function non_negative(name: string, value: float) -> KnobDomain throws never {
    KnobDomain {
        name,
        value: `${value}`,
        holds: value.is_finite() && value >= 0.0,
        domain: "a finite number no less than zero",
    }
}

function counting_from(name: string, value: int, low: int) -> KnobDomain throws never {
    KnobDomain {
        name,
        value: `${value}`,
        holds: value >= low,
        domain: `a whole number no less than ${low}`,
    }
}

/// Every numeric knob of `knobs` against its domain. Booleans have none.
///
/// Outside a domain the arithmetic does one of two things, and the second is
/// worse. `confidence` and `bar` reach `z_for` and a division, and panic. The
/// rest answer: a `slip` of two makes the chance of a right answer negative,
/// a `target` of one can never be reached so a sitting runs to its budget,
/// and every probability, every posterior and every draw downstream is then
/// a number that no longer means anything, with nothing to say so.
///
/// Finiteness is checked outright rather than left to the comparisons, which
/// NaN would pass: every comparison with it is false, including the ones a
/// bound is written as.
function knob_domains(knobs: Knobs) -> KnobDomain[] throws never {
    [
        open_unit("bar", knobs.bar),
        open_unit("confidence", knobs.confidence),
        open_unit("target", knobs.target),
        closed_unit("aim", knobs.aim),
        closed_unit("contrast", knobs.contrast),
        closed_unit("slip", knobs.slip),
        closed_unit("trap_slip", knobs.trap_slip),
        positive("band", knobs.band),
        non_negative("inform", knobs.inform),
        non_negative("exposure", knobs.exposure),
        non_negative("drift", knobs.drift),
        non_negative("suspect", knobs.suspect),
        non_negative("clear", knobs.clear),
        counting_from("budget", knobs.budget, 1),
        counting_from("stall_after", knobs.stall_after, 1),
        counting_from("minimum", knobs.minimum, 0),
        counting_from("rest", knobs.rest, 0),
        counting_from("recheck", knobs.recheck, 1),
    ]
}

/// Why `knobs` cannot be used, or `null` when they can: the first knob
/// outside its domain.
///
/// A surface whose knobs come from its own controls cannot reach a bad value.
/// One that takes them from anywhere else — a sitting saved by an older
/// build, a file someone edited — has to ask first, because what follows a
/// bad value is a panic or a silently meaningless estimate, and neither is
/// something it can show a learner. Asking here rather than restating the
/// bounds is what keeps the two from drifting apart.
function knobs_fault(knobs: Knobs) -> string? throws never {
    let broken = knob_domains(knobs).find((domain) -> {
        !domain.holds
    })
        ?? return null;
    `${broken.name} is ${broken.value}, and it has to be ${broken.domain}`
}

/// The lowest strength we would bet on for `skill`.
function floor_of(profile: Profile, knobs: Knobs, skill: Skill) -> float throws never {
    profile.ability
        + skill.offset
        - rule_z(profile, knobs) * baml.Float.sqrt(profile.variance + skill.variance)
}

/// The highest order at which we would bet on the learner for `skill`, among
/// orders they have been right at: zero when none. A rung is climbed by more
/// than a single right answer, or a learner is pushed under constructors
/// before the rule beneath has been settled.
///
/// This reads the ESTIMATE, not the floor. What to ask next is a bet, and a
/// bet is made on what we believe; the floor is a family-wise lower bound,
/// which is the standard for SAYING someone has a rule and much too strict
/// for choosing their next question. Reading the floor here means the ladder
/// never advances: the bound needed to call rung three climbed is that we are
/// ninety per cent sure, over every rule at once, that they hold each rule
/// nine times in ten — so a learner answering everything right is asked
/// one-rule cases to the end of the sitting, which is what three models who
/// sat the quiz each reported as the difficulty never climbing.
function climbed_rung(profile: Profile, knobs: Knobs, skill: Skill) -> int throws never {
    let held = logistic(profile.ability + skill.offset);
    baml.iter.Range.new(1, skill.highest_right + 1).reduce(
        (acc, order) -> {
            if (baml.Float.pow(held, order * 1.0) >= knobs.target) {
                order
            } else {
                acc
            }
        },
        0,
    )
}

/// A rule is mastered when we would bet on the learner applying it as
/// reliably as `target` asks, and they have in fact been right at the
/// deepest the bank takes it. Easy cases alone never certify a rule that can
/// appear in hard ones.
function mastered(profile: Profile, knobs: Knobs, skill: Skill) -> bool throws never {
    logistic(floor_of(profile, knobs, skill)) >= knobs.target && skill.highest_right >= skill.order
}

function mastered_rules(profile: Profile, knobs: Knobs) -> string[] throws never {
    profile.skills.filter_map((skill) -> {
        if (mastered(profile, knobs, skill)) {
            skill.rule
        } else {
            null
        }
    })
}

// ─── observing an answer ─────────────────────────────────────────────────────

/// An answer as the tracer sees it: which rules the case held (inner first),
/// which naive models it trapped, and what happened.
class Observed {
    /// The item the case came from, for exposure.
    item: string,
    rules: string[],
    traps: string[],
    /// Right, wrong, or withheld.
    verdict: bool?,
}

/// One parameter's posterior after an observation.
class Posterior {
    mean: float,
    variance: float,
}

/// The Gaussian update for one parameter: the mean moves by the score of the
/// log-likelihood and the variance falls by the information the outcome
/// carries about this parameter, both of which the chain rule takes through
/// `gradient`, the derivative of `q` in it.
///
/// A rule the learner already holds has a gradient near zero, so an answer
/// neither moves it nor narrows it. That is the whole of blame, derived.
function updated(
    mean: float,
    variance: float,
    gradient: float,
    score: float,
    information: float,
) -> Posterior throws never {
    let sensitivity = gradient * gradient;
    let after = 1.0 / (1.0 / variance + information * sensitivity);
    Posterior {
        // Bounded so a long run one way cannot drive the logistic flat, where
        // every gradient would vanish and no later answer could move it back.
        mean: (mean + after * score * gradient).max(0.0 - 8.0).min(8.0),
        variance: after,
    }
}

/// How many of `rules` are `rule`: a rule a case reaches twice must be
/// applied twice, and its gradient counts twice over.
function times_in(rules: string[], rule: string) -> float throws never {
    rules
        .filter((held) -> {
            held == rule
        })
        .length()
        * 1.0
}

/// The profile after `seen`. A pure function: the old profile is untouched.
function observe(profile: Profile, knobs: Knobs, seen: Observed) -> Profile throws never {
    let commit_rate = commits(profile, knobs);
    let q = knowing(profile, knobs, seen.rules, seen.traps.length());
    let chance = chances(q, knobs, commit_rate);
    let slope = slopes(knobs, commit_rate);
    // The score and the Fisher information of the outcome, both in `q`. The
    // information is what the answer was worth: an outcome the model already
    // expected carries almost none, and one at even odds carries the most.
    let score = ratio(taken(slope, seen.verdict), taken(chance, seen.verdict));
    let information = ratio(slope.right * slope.right, chance.right)
        + ratio(slope.wrong * slope.wrong, chance.wrong)
        + ratio(slope.withheld * slope.withheld, chance.withheld);
    // Every estimate widens a little before the answer narrows it, and no
    // wider than it was before anything was known.
    let widened = (profile.variance + knobs.drift).min(prior_variance());
    let doubts = seen.rules.map((rule) -> {
        1.0 - logistic(strength(profile, rule))
    });
    let total_doubt = doubts.reduce(
        (acc, doubt) -> {
            acc + doubt
        },
        0.0,
    );
    let global = updated(profile.ability, widened, q * total_doubt, score, information);
    let right = seen.verdict == true;
    let step = profile.step + 1;
    let skills = profile.skills.map((skill) -> {
        let held = logistic(profile.ability + skill.offset);
        let gradient = q * times_in(seen.rules, skill.rule) * (1.0 - held);
        let own = updated(
            skill.offset,
            (skill.variance + knobs.drift).min(prior_rule_variance()),
            gradient,
            score,
            information,
        );
        if (!seen.rules.includes(skill.rule)) {
            return Skill { ...skill, variance: own.variance };
        }
        Skill {
            ...skill,
            offset: own.mean,
            variance: own.variance,
            highest_right: if (right) {
                skill.highest_right.max(seen.rules.length())
            } else {
                skill.highest_right
            },
            wrong_streak: if (right) {
                0
            } else {
                skill.wrong_streak + 1
            },
            seen: skill.seen + 1,
            last_step: step,
        }
    });
    let suspicions = profile.suspicions.map((suspicion) -> {
        // A committed answer on a case that traps the model is evidence for
        // it by the shortfall against what the model expected BEFORE this
        // answer moved it: a one-step-ahead residual, so the sum is a
        // martingale and its spread is the sum of the per-answer variances.
        // A withheld answer is not evidence: the model predicts a particular
        // wrong verdict, and not knowing is not that.
        match (seen.verdict) {
            bool => {
                if (seen.traps.includes(suspicion.model)) {
                    let outcome = if (right) {
                        1.0
                    } else {
                        0.0
                    };
                    Suspicion {
                        model: suspicion.model,
                        score: suspicion.score + (chance.right - outcome),
                        variance: suspicion.variance + chance.right * (1.0 - chance.right),
                    }
                } else {
                    suspicion
                }
            },
            null => suspicion,
        }
    });
    let next = Profile {
        ability: global.mean,
        variance: global.variance,
        step,
        committed: profile.committed
            + (
                if (seen.verdict != null) {
                    1
                } else {
                    0
                }
            ),
        right: profile.right
            + (
                if (right) {
                    1
                } else {
                    0
                }
            ),
        held_back: profile.held_back
            + (
                if (seen.verdict == null) {
                    1
                } else {
                    0
                }
            ),
        unknown_mass: profile.unknown_mass + (1.0 - q),
        skills,
        suspicions,
        served: served_once_more(profile.served, seen.item),
        quiet: 0,
    };
    let after = next.skills.map((skill) -> {
        floor_of(next, knobs, skill)
    });
    let committed = seen.verdict != null;
    // Progress is whatever certifying a rule turns on: its FLOOR, and the
    // deepest order it has been applied right at. Two things this is not.
    // It is not the estimate, because a learner whose estimates have
    // converged while their uncertainty is still falling is still on their
    // way. And it is not the gain from ONE answer: with thirty rules to lift,
    // every answer's share of the work is smaller than the hair we would
    // notice, so a per-step threshold reads steady progress as a stall. The
    // gain is measured against the floor as of the last time we saw progress,
    // so it is the whole quiet stretch that has to have achieved nothing.
    let gained = baml.iter.Range.new(0, after.length()).some((i) -> {
        // Through a function: a method call whose receiver reads a captured
        // local bound from an earlier closure call miscompiles (README,
        // compiler issue 9).
        climbed(profile.skills[i], after[i])
            || deepened(profile.skills[i], next.skills[i])
            || (committed && first_met(profile.skills[i], next.skills[i]))
    });
    if (gained) {
        Profile {
            ...next,
            skills: baml.iter.Range.new(0, after.length())
                .map((i) -> {
                    Skill { ...next.skills[i], marked: after[i] }
                })
                .collect(),
            quiet: 0,
        }
    } else {
        Profile { ...next, quiet: profile.quiet + 1 }
    }
}

/// `served` with one more serving of `item`.
function served_once_more(served: map<string, int>, item: string) -> map<string, int> throws never {
    let counts: map<string, int> = {};
    for (let key in served.keys()) {
        counts.set(key, served.get(key) ?? 0);
    }
    counts.set(item, (served.get(item) ?? 0) + 1);
    counts
}

/// Whether `floor` stands more than a hair above where it was when this
/// sitting last made progress. An unmarked rule has nothing to stand above,
/// so the first answer of a sitting always counts.
function climbed(skill: Skill, floor: float) -> bool throws never {
    match (skill.marked) {
        let marked: float => floor - marked > 0.02,
        null => true,
    }
}

/// Whether the rule has now been applied right at an order it had not been
/// before, which is the other half of what certifying it asks.
function deepened(before: Skill, after: Skill) -> bool throws never {
    after.highest_right > before.highest_right
}

/// Whether a rule bore blame for the first time.
function first_met(before: Skill, after: Skill) -> bool throws never {
    before.seen == 0 && after.seen > 0
}

// ─── where a sitting stands ──────────────────────────────────────────────────

/// Why a sitting is over, or that it is not.
enum Why {
    Continue,
    Mastered,
    Budget,
    Stalled,
}

class Standing {
    why: Why,
    mastered: string[],
    /// Rules furthest below what we would bet on, worst first.
    shaky: string[],
    /// How often the learner commits to an answer they do not have,
    /// estimated from the sitting; `null` before there is anything to
    /// estimate it from. The bar asks for `1 - bar`, and THAT is the
    /// comparison, not how many answers were right: selection holds the
    /// share right near whatever it aims at, so accuracy measures the
    /// sitting rather than the learner.
    commits_unsure: float?,
    /// The share of answers withheld.
    withheld: float,
    /// The naive model the learner's errors most resemble, when one stands
    /// clear of the rest; `null` when none does.
    suspected: string?,
}

/// Whether the learner has shown enough, under the coverage the knobs ask for.
function enough(profile: Profile, knobs: Knobs) -> bool throws never {
    if (knobs.strict) {
        return profile.skills.every((skill) -> {
            mastered(profile, knobs, skill)
        });
    }
    // The level and the lag are two claims and are bounded separately. The
    // level is one test, so it takes the whole of `confidence`; a rule
    // lagging is twenty-five tests, so each takes its share. Comparing a
    // per-rule floor against a global one would compare bounds taken at
    // different confidences and always find the rule wanting.
    let global_floor = profile.ability - z_for(knobs.confidence) * baml.Float.sqrt(profile.variance);
    let lag = rule_z(profile, knobs);
    logistic(global_floor) >= knobs.target
        && profile.skills.every((skill) -> {
            skill.seen > 0
                && skill.offset - lag * baml.Float.sqrt(skill.variance) >= 0.0 - knobs.band
        })
}

function standing(profile: Profile, knobs: Knobs) -> Standing throws never {
    let why = if (knobs.practice) {
        if (profile.step >= knobs.budget) {
            Why.Budget
        } else {
            Why.Continue
        }
    } else if (enough(profile, knobs)) {
        Why.Mastered
    } else if (profile.step >= knobs.budget) {
        Why.Budget
    } else if (profile.step >= knobs.minimum && profile.quiet >= knobs.stall_after) {
        Why.Stalled
    } else {
        Why.Continue
    };
    // `sort_by_key` sorts in place, so it is given a copy.
    let by_z = profile.suspicions.slice(0, profile.suspicions.length()).sort_by_key((s) -> {
        0.0 - suspicion_z(s)
    });
    let runner_up = match (by_z.at(1)) {
        let s: Suspicion => suspicion_z(s),
        null => 0.0,
    };
    let suspected = match (by_z.at(0)) {
        let top: Suspicion => {
            let z = suspicion_z(top);
            if (z >= knobs.suspect && z - runner_up >= knobs.clear) {
                top.model
            } else {
                null
            }
        },
        null => null,
    };
    let shaky = profile.skills
        .filter((skill) -> {
            !mastered(profile, knobs, skill)
        })
        .sort_by_key((skill) -> {
            floor_of(profile, knobs, skill)
        })
        .map((skill) -> {
            skill.rule
        });
    Standing {
        why,
        mastered: mastered_rules(profile, knobs),
        shaky,
        commits_unsure: if (profile.step > 0) {
            commits(profile, knobs)
        } else {
            null
        },
        withheld: if (profile.step > 0) {
            (profile.step - profile.committed) * 1.0 / (profile.step * 1.0)
        } else {
            0.0
        },
        suspected,
    }
}

/// The rule to show the spec's own words for before the next case on it, or
/// `null` when none is stuck: the most-missed rule, if it has been missed
/// twice running.
function to_teach(profile: Profile) -> string? throws never {
    profile.skills
        .filter((skill) -> {
            skill.wrong_streak >= 2
        })
        .sort_by_key((skill) -> {
            0 - skill.wrong_streak
        })
        .at(0)
        ?.rule
}

// ─── the points ──────────────────────────────────────────────────────────────

/// What an answer is worth. Derived from the bar and nothing else: answering
/// beats abstaining exactly when the learner is surer than the bar, so the
/// score-maximising play is the honest one and there is nothing to game.
class Points {
    right: int,
    wrong: int,
    unsure: int,
}

/// # Panics
///
/// - `baml.panics.Unreachable` if the bar is not strictly between zero and
///   one, where no finite penalty could put the indifference point.
function points(knobs: Knobs) -> Points throws never {
    // With unsure worth nothing, answering wins when p * right > (1 - p) * wrong,
    // so the bar is wrong / (right + wrong): wrong = bar * right / (1 - bar).
    let right = 4.0;
    let wrong = (knobs.bar * right / (1.0 - knobs.bar)).iround() catch (e) {
        baml.errors.InvalidArgument => {
            throw baml.panics.Unreachable { message: `no finite penalty puts the bar at ${knobs.bar}` }
        },
    };
    Points { right: 4, wrong: 0 - wrong, unsure: 0 }
}

/// What one judged answer was worth: `verdict_correct` is absent when the
/// learner withheld.
function points_for(knobs: Knobs, verdict_correct: bool?) -> int throws never {
    let scale = points(knobs);
    match (verdict_correct) {
        let right: bool => {
            if (right) {
                scale.right
            } else {
                scale.wrong
            }
        },
        null => scale.unsure,
    }
}

// ─── choosing the next case ──────────────────────────────────────────────────

/// What one casting of a draw is worth to this learner: the information an
/// answer to it would carry, the chance they get it right, and every other
/// term of the fit.
class Assessed {
    information: float,
    success: float,
    terms: float,
}

/// How well a draw suits this learner now, on top of the bank's own score.
///
/// A draw is a lottery over its castings — the coins that decide which one
/// is served are not selection's to see, and they decide the answer — so
/// this is the lottery's worth: its expected information, or its expected
/// chance of success when aiming at one, and the mean of every other term.
/// Every casting of a draw is served equally often, so the mean is the
/// expectation. Reading the served casting instead would let the preference
/// follow the coin.
function fit(profile: Profile, knobs: Knobs, scored: Scored) -> float throws never {
    fit_from(worths(profile, knobs, [scored]), knobs, profile, scored)
}

/// `fit`, reading each casting's worth from `table` rather than working it
/// out again.
function fit_from(
    table: Worth[],
    knobs: Knobs,
    profile: Profile,
    scored: Scored,
) -> float throws never {
    let each = scored.candidate.castings.map((casting) -> {
        worth_of(table, casting)
    });
    let exposure = 0.0 - knobs.exposure * ((profile.served.get(scored.item) ?? 0) * 1.0);
    aim_of(
        knobs,
        mean(
            each.map((a) -> {
                a.information
            }),
        ),
        mean(
            each.map((a) -> {
                a.success
            }),
        ),
    )
        + exposure
        + mean(
            each.map((a) -> {
                a.terms
            }),
        )
}

/// Either the aim or the information, never both: they are two answers to
/// the same question. Drawing in proportion to information raised to
/// `inform` is a Boltzmann draw over what the answer is worth.
///
/// Its own function so the method calls land on parameters rather than on
/// locals bound from closure calls (README, compiler issue 9).
function aim_of(knobs: Knobs, information: float, success: float) -> float throws never {
    if (knobs.inform > 0.0) {
        knobs.inform * baml.Float.ln(information.max(0.000001))
    } else {
        0.0 - 3.0 * (success - knobs.aim).abs()
    }
}

/// What one casting would be worth if it were the one served.
function assessed(profile: Profile, knobs: Knobs, casting: Casting) -> Assessed throws never {
    let rules = casting.rules;
    let traps = casting.traps;
    // A case on top of a rule the learner has not got yet says nothing about
    // the rule on top.
    let inner_unready = rules.length() >= 2 && logistic(strength(profile, rules[0])) < 0.55;
    let readiness = if (inner_unready) {
        0.0 - 2.0
    } else {
        0.0
    };
    let all_mastered = rules.length() > 0
        && rules.every((rule) -> {
            match (skill_of(profile, rule)) {
                let skill: Skill => mastered(profile, knobs, skill),
                null => false,
            }
        });
    let due = rules.some((rule) -> {
        match (skill_of(profile, rule)) {
            let skill: Skill => {
                skill.last_step > 0 && profile.step - skill.last_step >= knobs.recheck
            },
            null => false,
        }
    });
    let retired = if (all_mastered && !due) {
        0.0 - 3.0
    } else {
        0.0
    };
    let fresh_enough = rules.every((rule) -> {
        match (skill_of(profile, rule)) {
            let skill: Skill => skill.last_step == 0 || profile.step - skill.last_step >= knobs.rest,
            null => true,
        }
    });
    let recency = if (fresh_enough) {
        0.0
    } else {
        0.0 - 1.5
    };
    let suspicion = profile.suspicions
        .filter((s) -> {
            traps.includes(s.model)
        })
        .reduce(
            (acc, s) -> {
                acc.max(suspicion_z(s))
            },
            0.0,
        );
    let teaching = match (to_teach(profile)) {
        let rule: string => {
            if (rules.includes(rule)) {
                1.0
            } else {
                0.0
            }
        },
        null => 0.0,
    };
    // Climb one rung at a time: a rule is best asked at the order just above
    // the highest it has been right at. Below that is a rung already climbed;
    // above it skips one, and a failure there says nothing about which rung
    // gave way. This is the difficulty curve, made from the rule orders the
    // bank derives rather than from a level anyone assigned.
    let order = rules.length();
    let ladder = rules.reduce(
        (acc, rule) -> {
            let skill = skill_of(profile, rule);
            let climbed = match (skill) {
                let s: Skill => climbed_rung(profile, knobs, s),
                null => 0,
            };
            // The next rung is the rule's lowest order until that is climbed,
            // then one above the highest climbed so far.
            let next_rung = if (climbed == 0) {
                skill?.lowest ?? 1
            } else {
                climbed + 1
            };
            if (order == next_rung) {
                acc + 1.0
            } else if (order > next_rung) {
                acc - 1.5
            } else {
                acc
            }
        },
        0.0,
    );
    // Seek what we know least about: the classic active-learning term.
    // Pooling holds the per-rule variances close together, so on its own it
    // separates cases only weakly; a rule nobody has been asked about is
    // called out by name, since certification asks for every rule to have
    // been met and a sitting would otherwise stall with one untouched.
    let unknown = rules.reduce(
        (acc, rule) -> {
            acc.max(baml.Float.sqrt(skill_of(profile, rule)?.variance ?? 0.0))
        },
        0.0,
    );
    let unmet = rules.some((rule) -> {
        match (skill_of(profile, rule)) {
            let skill: Skill => skill.seen == 0,
            null => false,
        }
    });
    let reaching = if (unmet) {
        3.0
    } else {
        0.0
    };
    // Finish what was started: a rule already met but not yet mastered is
    // worth more than one never met, or a sitting spreads itself over every
    // rule at once and brings none of them home.
    let underway = rules.some((rule) -> {
        match (skill_of(profile, rule)) {
            let skill: Skill => skill.seen > 0 && !mastered(profile, knobs, skill),
            null => false,
        }
    });
    let finishing = if (underway) {
        1.5
    } else {
        0.0
    };
    Assessed {
        information: information(profile, knobs, rules, traps.length()),
        success: predict(profile, knobs, rules, traps.length()),
        terms: readiness
            + retired
            + recency
            + 0.5 * suspicion.min(4.0)
            + teaching
            + 2.0 * unknown
            + reaching
            + ladder
            + finishing,
    }
}

/// A casting's worth, keyed by the only parts of it that worth reads.
class Worth {
    rules: string[],
    traps: string[],
    assessed: Assessed,
}

/// What each distinct casting among `scored` is worth, each worked out once.
///
/// A casting's worth turns on its rules and its traps alone, and the same
/// ones recur across a batch — both directions of a pair that fools nobody,
/// the same fact drawn twice — so this is most of what selection would
/// otherwise repeat. Nothing about the answer changes: the same function
/// meets the same inputs.
function worths(profile: Profile, knobs: Knobs, scored: Scored[]) -> Worth[] throws never {
    let table: Worth[] = [];
    for (let s in scored) {
        for (let casting in s.candidate.castings) {
            let known = table.some((worth) -> {
                worth.rules == casting.rules && worth.traps == casting.traps
            });
            if (!known) {
                table.push(
                    Worth {
                        rules: casting.rules,
                        traps: casting.traps,
                        assessed: assessed(profile, knobs, casting),
                    },
                );
            }
        }
    }
    table
}

/// `casting`'s worth as `table` holds it.
///
/// # Panics
///
/// - `baml.panics.Unreachable` if `table` was not built from a set of draws
///   that included `casting`.
function worth_of(table: Worth[], casting: Casting) -> Assessed throws never {
    let found = table.find((worth) -> {
        worth.rules == casting.rules && worth.traps == casting.traps
    })
        ?? throw baml.panics.Unreachable { message: "a casting its table was not built from" };
    found.assessed
}

/// One of `scored`, drawn proportional to `exp(bank score + fit)`.
function choose(
    profile: Profile,
    knobs: Knobs,
    scored: Scored[],
    seed: Seed,
) -> Scored? throws never {
    let table = worths(profile, knobs, scored);
    pick(
        scored.map((s) -> {
            Scored { ...s, score: s.score + fit_from(table, knobs, profile, s) }
        }),
        seed,
    )
}
