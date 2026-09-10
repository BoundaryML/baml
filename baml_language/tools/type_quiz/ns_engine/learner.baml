// What we believe about a learner, and how a sitting should go for them.
//
// The model is Elo-style knowledge tracing with partial pooling: one global
// ability, and a deviation per rule that shrinks toward it. Every answer moves
// the global estimate, which stands in for rules barely tested, so certainty
// is reached in a fraction of the questions that judging each rule on its own
// would take. Recency needs no window: an update is a filter, so what happened
// lately dominates and early mistakes wash out.
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
    /// Deviation from the global ability, in logits. Shrinks toward zero, so a
    /// rule with little evidence borrows the global estimate.
    offset: float,
    /// Uncertainty of `offset`, as a standard deviation. Shrinks with evidence.
    spread: float,
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
class Suspicion {
    model: string,
    score: float,
}

class Profile {
    /// Global ability, in logits.
    ability: float,
    /// Uncertainty of `ability`, as a standard deviation.
    spread: float,
    /// Observations so far, withheld answers included.
    step: int,
    /// Answers that gave a verdict.
    committed: int,
    /// Committed answers that were right.
    right: int,
    skills: Skill[],
    suspicions: Suspicion[],
    /// Consecutive observations in which no rule's estimate rose by more
    /// than a hair: the learner is gaining ground nowhere. The estimate, not
    /// the floor: shrinking uncertainty raises a floor even on bad news, and
    /// a learner who withholds on everything would otherwise never be called
    /// stuck.
    quiet: int,
}

/// Every tunable of the model, in one place.
class Knobs {
    /// The confidence below which a learner should say they are not sure.
    /// Sets the guess rate a committed answer is read with, and the points.
    bar: float,
    /// The success probability a rule must reach, at its highest order, to
    /// count as mastered.
    target: float,
    /// How many standard deviations below the estimate mastery is judged at.
    confidence: float,
    /// Coverage: every rule mastered on its own (`true`), or the global
    /// estimate confident with no rule further than `band` below it.
    strict: bool,
    band: float,
    /// The most questions a sitting may ask.
    budget: int,
    /// Stop after this many observations that moved no rule's floor, once
    /// at least `minimum` have been made: a sitting is given time to reach
    /// every rule before it can be called stuck.
    stall_after: int,
    minimum: int,
    /// The success probability selection aims a case at.
    aim: float,
    /// Rules seen within this many steps are avoided.
    rest: int,
    /// A mastered rule is rechecked after this many steps.
    recheck: int,
}

function default_knobs() -> Knobs throws never {
    Knobs {
        bar: 0.75,
        target: 0.85,
        confidence: 1.28,
        strict: false,
        band: 0.8,
        budget: 80,
        stall_after: 20,
        minimum: 40,
        aim: 0.8,
        rest: 3,
        recheck: 15,
    }
}

/// A profile that has seen nothing.
function fresh(rules: RuleOrder[], models: string[]) -> Profile throws never {
    Profile {
        ability: 0.0,
        spread: 1.5,
        step: 0,
        committed: 0,
        right: 0,
        skills: rules.map((r) -> {
            Skill {
                rule: r.rule,
                lowest: r.lowest,
                order: r.highest,
                offset: 0.0,
                spread: 1.0,
                highest_right: 0,
                wrong_streak: 0,
                seen: 0,
                last_step: 0,
            }
        }),
        suspicions: models.map((model) -> {
            Suspicion { model, score: 0.0 }
        }),
        quiet: 0,
    }
}

// ─── the arithmetic ──────────────────────────────────────────────────────────

function logistic(x: float) -> float throws never {
    1.0 / (1.0 + baml.Float.exp(0.0 - x))
}

function logit(p: float) -> float throws never {
    baml.Float.ln(p / (1.0 - p))
}

/// The chance a learner who does not know still answers correctly: a coin
/// flip, times the share of unsure learners the bar leaves committing.
function guess(knobs: Knobs) -> float throws never {
    0.5 * (1.0 - knobs.bar)
}

/// How hard a case is, in logits, from its order and how many naive models it
/// traps.
function difficulty(order: int, traps: int) -> float throws never {
    0.8 * (order - 1) * 1.0 + 0.3 * traps * 1.0
}

function skill_of(profile: Profile, rule: string) -> Skill? throws never {
    profile.skills.find((skill) -> {
        skill.rule == rule
    })
}

/// The learner's strength at `rule`, in logits: ability plus deviation.
function strength(profile: Profile, rule: string) -> float throws never {
    profile.ability + (skill_of(profile, rule)?.offset ?? 0.0)
}

/// The chance of a correct committed answer on a case of these rules.
function predict(
    profile: Profile,
    knobs: Knobs,
    rules: string[],
    traps: int,
) -> float throws never {
    if (rules.length() == 0) {
        return guess(knobs);
    }
    let mean = rules.reduce(
        (acc, rule) -> {
            acc + strength(profile, rule)
        },
        0.0,
    )
        / (rules.length() * 1.0);
    let g = guess(knobs);
    g + (1.0 - g) * logistic(mean - difficulty(rules.length(), traps))
}

/// The lowest strength we would bet on for `skill`, `knobs.confidence`
/// deviations below the estimate.
function floor_of(profile: Profile, knobs: Knobs, skill: Skill) -> float throws never {
    let variance = profile.spread * profile.spread + skill.spread * skill.spread;
    profile.ability + skill.offset - knobs.confidence * baml.Float.sqrt(variance)
}

/// The highest order at which we would bet on the learner for `skill`, among
/// orders they have been right at: zero when none. A rung is climbed by
/// certainty, not by a single right answer, or a learner is pushed under
/// constructors before the rule beneath has been settled.
function climbed_rung(profile: Profile, knobs: Knobs, skill: Skill) -> int throws never {
    let g = guess(knobs);
    let floor = floor_of(profile, knobs, skill);
    baml.iter.Range.new(1, skill.highest_right + 1).reduce(
        (acc, order) -> {
            let p = g + (1.0 - g) * logistic(floor - difficulty(order, 0));
            if (p >= knobs.target) {
                order
            } else {
                acc
            }
        },
        0,
    )
}

/// A rule is mastered when we would bet on the learner at its highest order,
/// and they have in fact been right there at least once. Easy cases alone
/// never certify a rule that can appear in hard ones.
function mastered(profile: Profile, knobs: Knobs, skill: Skill) -> bool throws never {
    let g = guess(knobs);
    let p = g + (1.0 - g) * logistic(floor_of(profile, knobs, skill) - difficulty(skill.order, 0));
    p >= knobs.target && skill.highest_right >= skill.order
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
    rules: string[],
    traps: string[],
    /// Right, wrong, or withheld.
    verdict: bool?,
}

/// The share of a wrong answer each rule bears: the least-known rule the most,
/// so a well-established rule is not punished for a new one's failure. A
/// withheld answer on a wrapped case is read as being about the wrapping: the
/// learner met an axiom under a constructor and would not commit, which says
/// nothing about the axiom on its own, so the outermost rule takes it all.
function blame(
    profile: Profile,
    rules: string[],
    right: bool,
    withheld: bool,
) -> float[] throws never {
    let n = rules.length() * 1.0;
    if (right) {
        return rules.map((rule) -> {
            1.0
        });
    }
    if (withheld && rules.length() > 1) {
        return rules.map((rule) -> {
            if (rule == rules[rules.length() - 1]) {
                n
            } else {
                0.0
            }
        });
    }
    let doubts = rules.map((rule) -> {
        1.0 - logistic(strength(profile, rule))
    });
    let total = doubts.reduce(
        (acc, d) -> {
            acc + d
        },
        0.0,
    );
    doubts.map((d) -> {
        if (total > 0.0) {
            d * n / total
        } else {
            1.0
        }
    })
}

/// The profile after `seen`. A pure function: the old profile is untouched.
function observe(profile: Profile, knobs: Knobs, seen: Observed) -> Profile throws never {
    let before = profile.skills.map((skill) -> {
        profile.ability + skill.offset
    });
    let p = predict(profile, knobs, seen.rules, seen.traps.length());
    // A withheld answer says the learner did not know, not that they believed
    // the opposite: a fraction of a wrong answer's weight, and it never counts
    // as committed.
    let weight = match (seen.verdict) {
        bool => 1.0,
        null => 0.3,
    };
    let outcome = match (seen.verdict) {
        let right: bool => {
            if (right) {
                1.0
            } else {
                0.0
            }
        },
        null => 0.0,
    };
    let surprise = (outcome - p) * weight;
    let right = seen.verdict == true;
    let shares = blame(profile, seen.rules, right, seen.verdict == null);
    let step = profile.step + 1;
    let gain = profile.spread / (profile.spread + 1.0);
    let ability = profile.ability + 2.0 * gain * surprise;
    let spread = (profile.spread * 0.92).max(0.25);
    let skills = profile.skills.map((skill) -> {
        let at = seen.rules.index_of(skill.rule);
        match (at) {
            let i: int => {
                // A rule that bore none of the blame gathered no evidence: its
                // spread does not shrink and it does not count as seen, or an
                // axiom met only under constructors the learner withheld on
                // would read as well-studied while nothing about it was learnt.
                if (shares[i] == 0.0) {
                    return skill;
                }
                let own_gain = skill.spread / (skill.spread + 1.0);
                let moved = skill.offset + 2.0 * own_gain * surprise * shares[i];
                Skill {
                    ...skill,
                    // Shrinking toward zero is the pooling: a rule with little
                    // evidence stays close to the global estimate.
                    offset: moved * 0.97,
                    spread: (skill.spread * 0.85).max(0.2),
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
            },
            null => skill,
        }
    });
    let suspicions = profile.suspicions.map((suspicion) -> {
        if (seen.traps.includes(suspicion.model)) {
            // Answering as the model would is evidence for it; beating it is
            // evidence against, less strongly, since a right answer has a
            // guess in it.
            let moved = match (seen.verdict) {
                let r: bool => {
                    if (r) {
                        suspicion.score - 0.5
                    } else {
                        suspicion.score + 1.0
                    }
                },
                null => suspicion.score + 0.25,
            };
            Suspicion { model: suspicion.model, score: moved.max(0.0) }
        } else {
            suspicion
        }
    });
    let next = Profile {
        ability,
        spread,
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
        skills,
        suspicions,
        quiet: 0,
    };
    let after = next.skills.map((skill) -> {
        next.ability + skill.offset
    });
    let gained = baml.iter.Range.new(0, after.length()).some((i) -> {
        // Through a function: a method call whose receiver reads a captured
        // local bound from an earlier closure call miscompiles (README,
        // compiler issue 9).
        rose(before[i], after[i])
    });
    Profile {
        ...next,
        quiet: if (gained) {
            0
        } else {
            profile.quiet + 1
        },
    }
}

/// Whether an estimate rose by more than a hair.
function rose(before: float, after: float) -> bool throws never {
    after - before > 0.02
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
    /// Of committed answers, the share that were right; compare to the bar.
    calibration: float?,
    /// The share of answers withheld.
    withheld: float,
}

/// Whether the learner has shown enough, under the coverage the knobs ask for.
function enough(profile: Profile, knobs: Knobs) -> bool throws never {
    if (knobs.strict) {
        return profile.skills.every((skill) -> {
            mastered(profile, knobs, skill)
        });
    }
    let g = guess(knobs);
    let global_floor = profile.ability - knobs.confidence * profile.spread;
    let global_ok = g + (1.0 - g) * logistic(global_floor - difficulty(2, 0)) >= knobs.target;
    global_ok
        && profile.skills.every((skill) -> {
            skill.seen > 0 && floor_of(profile, knobs, skill) >= global_floor - knobs.band
        })
}

function standing(profile: Profile, knobs: Knobs) -> Standing throws never {
    let why = if (enough(profile, knobs)) {
        Why.Mastered
    } else if (profile.step >= knobs.budget) {
        Why.Budget
    } else if (profile.step >= knobs.minimum && profile.quiet >= knobs.stall_after) {
        Why.Stalled
    } else {
        Why.Continue
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
        calibration: if (profile.committed > 0) {
            profile.right * 1.0 / (profile.committed * 1.0)
        } else {
            null
        },
        withheld: if (profile.step > 0) {
            (profile.step - profile.committed) * 1.0 / (profile.step * 1.0)
        } else {
            0.0
        },
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

// ─── choosing the next case ──────────────────────────────────────────────────

/// A candidate with the rules the tracer reads it by.
class Read {
    scored: Scored,
    rules: string[],
}

/// How well a case suits this learner now, on top of the bank's own score.
function fit(profile: Profile, knobs: Knobs, read: Read) -> float throws never {
    let rules = read.rules;
    let traps = read.scored.candidate.traps;
    let p = predict(profile, knobs, rules, traps.length());
    let aim = 0.0 - 3.0 * (p - knobs.aim).abs();
    // A case on top of a rule the learner has not got yet says nothing about
    // the rule on top.
    let inner_unready = rules.length() >= 2 && logistic(strength(profile, rules[0]) - difficulty(1, 0)) < 0.55;
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
                acc.max(s.score)
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
    // Seek what we know least about. This is the classic active-learning
    // term, and the only force that ever brings a rule nobody has been asked
    // about to the front; without it a sitting settles on the rules it has
    // already seen and stalls with the rest untouched.
    let unknown = rules.reduce(
        (acc, rule) -> {
            acc.max(skill_of(profile, rule)?.spread ?? 0.0)
        },
        0.0,
    );
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
    aim
        + readiness
        + retired
        + recency
        + 0.5 * suspicion.min(4.0)
        + teaching
        + 2.0 * unknown
        + ladder
        + finishing
}

/// One of `reads`, drawn proportional to `exp(bank score + fit)`.
function choose(profile: Profile, knobs: Knobs, reads: Read[], seed: Seed) -> Read? throws never {
    let rescored = reads.map((read) -> {
        Scored { ...read.scored, score: read.scored.score + fit(profile, knobs, read) }
    });
    let picked = pick(rescored, seed)
        ?? return null;
    reads.find((read) -> {
        read.scored.item == picked.item && read.scored.seed == picked.seed
    })
}
