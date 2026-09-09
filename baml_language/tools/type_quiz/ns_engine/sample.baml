// Choosing which case to show: every candidate gets a score from its
// features and a weight table, a hard cap rejects it outright, and the pick
// is proportional to score. Weights are the only knob; the learner profile
// will supply them later, and a session alternates the verdict it wants so
// that the answer is never predictable from how interesting a case looks.

/// The shape of a feature's contribution: nothing up to `allowance`, then
/// quadratic in the excess, and rejected outright above `hard_cap`.
class Hinge {
    allowance: float,
    hard_cap: float?,
}

class Weight {
    weight: float,
    hinge: Hinge?,
}

class Weights {
    entries: map<string, Weight>,
}

/// The weighted sum of the shaped features, or `null` when a hard cap is
/// exceeded. A feature absent from `features` reads as zero.
function score(features: map<string, float>, weights: Weights) -> float? throws never {
    let total = 0.0;
    for (let name in weights.entries.keys()) {
        let entry = weights.entries[name];
        let value = features.get(name) ?? 0.0;
        let shaped = match (entry.hinge) {
            let h: Hinge => {
                if let cap: float = h.hard_cap {
                    if (value > cap) {
                        return null;
                    }
                }
                let excess = value - h.allowance;
                if (excess > 0.0) {
                    excess * excess
                } else {
                    0.0
                }
            },
            null => value,
        };
        total = total + entry.weight * shaped;
    }
    total
}

class Scored {
    candidate: Candidate,
    score: float,
}

/// `count` candidates of `item` from child streams of `seed`, scored, with
/// the capped ones dropped.
function draw(item: Item, seed: Seed, weights: Weights, count: int) -> Scored[] throws never {
    let out: Scored[] = [];
    let s = seed;
    let i = 0;
    while (i < count) {
        let streams = split(s);
        s = streams.left;
        if let candidate: Candidate = item.generate(streams.right) {
            if let value: float = score(candidate.features, weights) {
                let _ = out.push(Scored { candidate: candidate, score: value });
            }
        }
        i = i + 1;
    }
    out
}

/// One of `scored`, drawn with probability proportional to `exp(score)`.
///
/// Written with loops rather than closures: a closure that captures a local
/// assigned from an earlier closure call (here `top`, from a `reduce`) reads
/// the wrong slot at run time. Minimal repro in the README's issue list.
function pick(scored: Scored[], seed: Seed) -> Scored? throws never {
    if (scored.length() == 0) {
        return null;
    }
    let top = scored[0].score;
    let i = 0;
    while (i < scored.length()) {
        if (scored[i].score > top) {
            top = scored[i].score;
        }
        i = i + 1;
    }
    let weights: float[] = [];
    let total = 0.0;
    i = 0;
    while (i < scored.length()) {
        let w = (scored[i].score - top).exp();
        let _ = weights.push(w);
        total = total + w;
        i = i + 1;
    }
    let u = unit(seed) * total;
    let cumulative = 0.0;
    i = 0;
    while (i < scored.length()) {
        cumulative = cumulative + weights[i];
        if (u < cumulative) {
            return scored[i];
        }
        i = i + 1;
    }
    scored[scored.length() - 1]
}

/// A draw in `[0, 1)`.
function unit(seed: Seed) -> float throws never {
    let d = below(seed, 1073741824);
    d.value * 1.0 / 1073741824.0
}

function is_compiling(candidate: Candidate) -> bool throws never {
    match (candidate.case.key) {
        Compiles => true,
        Rejected => false,
    }
}

/// A session's worth of cases. Each step wants the opposite verdict from the
/// last, draws from `per_step` seeded items, and picks proportional to score
/// among candidates with the wanted verdict, falling back to any candidate
/// when none has it. The alternation is what keeps the verdict independent
/// of everything the score rewards.
function session(
    items: Item[],
    seed: Seed,
    weights: Weights,
    length: int,
    per_step: int,
    draws: int,
) -> Scored[] throws never {
    let out: Scored[] = [];
    let s = seed;
    let step = 0;
    while (step < length && items.length() > 0) {
        let want_compiling = step % 2 == 0;
        let pool: Scored[] = [];
        let j = 0;
        while (j < per_step) {
            let choice = below(s, items.length());
            let streams = split(choice.next);
            s = streams.left;
            for (let scored in draw(items[choice.value], streams.right, weights, draws)) {
                let _ = pool.push(scored);
            }
            j = j + 1;
        }
        let wanted = pool.filter((c) -> {
            is_compiling(c.candidate) == want_compiling
        });
        let streams = split(s);
        s = streams.left;
        let chosen = pick(
            if (wanted.length() > 0) {
                wanted
            } else {
                pool
            },
            streams.right,
        );
        if (chosen != null) {
            let _ = out.push(chosen);
        }
        step = step + 1;
    }
    out
}
