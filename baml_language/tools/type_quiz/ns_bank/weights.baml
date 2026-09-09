// The default weight table: hand-set until a learner profile supplies one.
// Positive weights reward interest; size features carry a negative weight
// behind a hinge, so small cases are free and large ones pay quadratically
// until a hard cap rejects them.
//
// `boring` (a single rule every model agrees with) is a soft penalty, not a
// gate. Small rejected cases are mostly boring under today's models, so a
// gate would leave the session unable to balance the verdict among small
// cases; a learner profile can also want these as axiom drills.

function weight(weight: float) -> root.engine.Weight throws never {
    root.engine.Weight { weight: weight, hinge: null }
}

function penalty(
    weight: float,
    allowance: float,
    hard_cap: float,
) -> root.engine.Weight throws never {
    root.engine.Weight {
        weight: weight,
        hinge: root.engine.Hinge { allowance: allowance, hard_cap: hard_cap },
    }
}

function default_weights() -> root.engine.Weights throws never {
    root.engine.Weights {
        entries: {
            "rules": weight(1.0),
            "flips": weight(1.5),
            "traps": weight(2.0),
            "size": penalty(-0.05, 10.0, 40.0),
            "depth": penalty(-0.5, 3.0, 6.0),
            "width": penalty(-0.2, 3.0, 8.0),
            "boring": weight(-2.0),
        },
    }
}
