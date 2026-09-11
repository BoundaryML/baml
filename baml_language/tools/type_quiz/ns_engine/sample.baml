// Choosing which case to show: every candidate gets a score from its
// features and a weight table, a hard cap rejects it outright, and the pick
// is proportional to `exp(score)`. Weights are the only knob; the learner
// profile will supply them later. A session fixes each step's verdict before
// it looks at a candidate, from a seeded balanced schedule, so the answer is
// independent of everything the score rewards and is not predictable from
// the step's index or from the previous answer.

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

/// `value` shaped by `hinge`, or `null` when it exceeds the hard cap. Without
/// a hinge the value is its own shape.
function shape(hinge: Hinge?, value: float) -> float? throws never {
    let h = hinge
        ?? return value;
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
}

/// The weighted sum of the shaped features, or `null` when a hard cap is
/// exceeded. A feature absent from `features` reads as zero.
function score(features: map<string, float>, weights: Weights) -> float? throws never {
    let start: float? = 0.0;
    weights.entries.keys().reduce(
        (total, name) -> {
            let acc = total
                ?? return null;
            let entry = weights.entries[name];
            let shaped = shape(entry.hinge, features.get(name) ?? 0.0)
                ?? return null;
            acc + entry.weight * shaped
        },
        start,
    )
}

/// A candidate with where it came from and its score. The item and seed are
/// carried so a case can be generated again from a written-down sitting.
class Scored {
    item: string,
    seed: Seed,
    candidate: Candidate,
    score: float,
}

/// `count` child streams of `seed`: the right-hand stream of each successive
/// split.
function streams(seed: Seed, count: int) -> Seed[] throws never {
    let none: Seed[] = [];
    baml.iter.Range.new(0, count)
        .reduce(
            (acc, i) -> {
                let pair = split(acc.next);
                Rolled<Seed[]> { value: acc.value.concat([pair.right]), next: pair.left }
            },
            Rolled<Seed[]> { value: none, next: seed },
        )
        .value
}

/// `item.generate(seed)` behind a function, so a closure can call it: the
/// emitter panics on an interface method called on a captured value inside a
/// closure (README, compiler issue 1), and a call through a function does
/// not capture.
function generate(item: Item, seed: Seed) -> Candidate? throws never {
    item.generate(seed)
}

/// `count` candidates of `item` from child streams of `seed`, scored, with
/// the capped ones dropped.
function draw(item: Item, seed: Seed, weights: Weights, count: int) -> Scored[] throws never {
    let id = item.id();
    streams(seed, count).filter_map((s) -> {
        let candidate = generate(item, s)
            ?? return null;
        let value = score(candidate.features, weights)
            ?? return null;
        Scored { item: id, seed: s, candidate, score: value }
    })
}

/// A cumulative walk over the candidates' weights: the running total and the
/// first candidate whose total passed the draw.
class Walk {
    cumulative: float,
    chosen: Scored?,
}

/// One of `scored`, drawn with probability proportional to `exp(score)`, or
/// `null` when there is nothing to draw from. Scores are shifted by their
/// maximum first so every weight is finite.
function pick(scored: Scored[], seed: Seed) -> Scored? throws never {
    let first = scored.at(0)
        ?? return null;
    let top = scored.reduce(
        (acc, s) -> {
            acc.max(s.score)
        },
        first.score,
    );
    let total = scored.reduce(
        (acc, s) -> {
            acc + baml.Float.exp(s.score - top)
        },
        0.0,
    );
    let u = unit(seed) * total;
    let walk = scored.reduce(
        (acc, s) -> {
            match (acc.chosen) {
                Scored => acc,
                null => {
                    let cumulative = acc.cumulative + baml.Float.exp(s.score - top);
                    Walk {
                        cumulative,
                        chosen: if (u < cumulative) {
                            s
                        } else {
                            null
                        },
                    }
                },
            }
        },
        Walk { cumulative: 0.0, chosen: null },
    );
    // Rounding can leave `u` at or above the final total; the last candidate
    // is then the draw.
    walk.chosen ?? scored.at(-1)
}

/// A draw in `[0, 1)`.
function unit(seed: Seed) -> float throws never {
    below(seed, 1073741824).value * 1.0 / 1073741824.0
}

// ─── sessions ────────────────────────────────────────────────────────────────

/// `length` verdict kinds, half of each (the odd one compiles), in a seeded
/// order. The order is a uniform shuffle, so the kind at a step is not
/// predictable from the step's index or its predecessors; the balance is
/// exact by construction, so any imbalance among served cases is the
/// fallbacks' doing.
function schedule(seed: Seed, length: int) -> Rolled<VerdictKind[]> throws never {
    let balanced: VerdictKind[] = baml.iter.Range.new(0, length)
        .map((i) -> {
            if (i % 2 == 0) {
                VerdictKind.Compiles
            } else {
                VerdictKind.Rejected
            }
        })
        .collect();
    pick_distinct(seed, balanced, length)
}

/// A step of a session with a case to show: what the schedule wanted and
/// what was picked. The pick has the wanted verdict unless the step fell
/// back.
class Served {
    wanted: VerdictKind,
    chosen: Scored,
}

/// A step with nothing to show: every candidate drawn was over a hard cap,
/// or there was no item to draw from.
class Starved {
    wanted: VerdictKind,
}

type Turn = Served | Starved;

/// Whether the step had to settle for the other verdict.
function fell_back(turn: Served) -> bool throws never {
    verdict_kind(turn.chosen.candidate.case.key) != turn.wanted
}

/// The steps of `turns` that have a case, in order.
function served(turns: Turn[]) -> Served[] throws never {
    turns.filter_map((turn) -> {
        match (turn) {
            let s: Served => s,
            Starved => null,
        }
    })
}

/// How much a turn may draw: `per_step` items with `draws` candidates each
/// per batch, and up to `redraws` further batches while no candidate drawn
/// so far has the wanted verdict.
class Budget {
    per_step: int,
    draws: int,
    redraws: int,
}

/// One step's plan: the verdict wanted and the stream to draw with.
class Planned {
    wanted: VerdictKind,
    seed: Seed,
}

/// A session of `length` turns. Each turn draws a batch of candidates, and
/// further batches within the budget while none has the scheduled verdict,
/// then picks among those that have it proportional to score. Only when the
/// budget is spent without one does the turn fall back to the whole pool.
///
/// Only `Verified` items are served: the quiz presents the compiler's answer
/// as the truth, so an item the compiler is known to disagree with has no
/// place in a session.
function session(
    items: Item[],
    seed: Seed,
    weights: Weights,
    length: int,
    budget: Budget,
) -> Turn[] throws never {
    let verified = items.filter((item) -> {
        item.status() is Verified
    });
    let plan = schedule(seed, length);
    let step_seeds = streams(plan.next, length);
    let planned: Planned[] = baml.iter.Range.new(0, length)
        .map((i) -> {
            Planned { wanted: plan.value[i], seed: step_seeds[i] }
        })
        .collect();
    planned.map((p) -> {
        turn(verified, p, weights, budget)
    })
}

/// The candidates a turn has drawn so far, and those among them with the
/// wanted verdict.
class Drawn {
    pool: Scored[],
    wanted: Scored[],
}

function turn(
    items: Item[],
    planned: Planned,
    weights: Weights,
    budget: Budget,
) -> Turn throws never {
    let seeds = split(planned.seed);
    let none: Scored[] = [];
    let drawn = streams(seeds.left, budget.redraws + 1).reduce(
        (acc, batch_seed) -> {
            if (acc.wanted.length() > 0) {
                return acc;
            }
            let pool = acc.pool.concat(batch(items, batch_seed, weights, budget));
            Drawn {
                pool,
                wanted: pool.filter((c) -> {
                    verdict_kind(c.candidate.case.key) == planned.wanted
                }),
            }
        },
        Drawn { pool: none, wanted: none },
    );
    let candidates = if (drawn.wanted.length() > 0) {
        drawn.wanted
    } else {
        drawn.pool
    };
    match (pick(candidates, seeds.right)) {
        let chosen: Scored => Served { wanted: planned.wanted, chosen },
        null => Starved { wanted: planned.wanted },
    }
}

/// `item`'s foil at `seed`. Through a function for the same reason as
/// `generate`: an interface method on a captured existential dies at emit
/// time (README, compiler issue 1).
function foil_of(item: Item, seed: Seed) -> Foil? throws never {
    item.foil(seed)
}

/// `budget.draws` candidates from each of `budget.per_step` items drawn
/// from `seed` with replacement; nothing when there are no items.
function batch(
    items: Item[],
    seed: Seed,
    weights: Weights,
    budget: Budget,
) -> Scored[] throws never {
    let count = items.length();
    if (count == 0) {
        return [];
    }
    streams(seed, budget.per_step).flat_map((s) -> {
        let choice = below(s, count);
        draw(items[choice.value], choice.next, weights, budget.draws)
    })
}
