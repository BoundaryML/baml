// Putting cases to a learner.
//
// The engine hands out questions and judges answers; it owns no input or
// output and no loop. Whoever is asking — a page, a script — keeps the
// answers and decides when to ask the next one, which is the only shape that
// suits a user interface, where the next question is a click away rather than
// a line away.
//
// A sitting is a pure function of its seed, so a question can be worked out
// again whenever it is wanted instead of being held somewhere in the
// meantime. That is what lets a learner be handed a program without also
// being handed its answer.

function question_of(turn: Served) -> Question throws never {
    Question {
        item: turn.chosen.item,
        seed: turn.chosen.seed.state,
        case: turn.chosen.candidate.case,
    }
}

/// The questions of a sitting, in order.
///
/// The same seed plans the same sitting, so this is worth calling again
/// rather than storing: a learner can be given the same quiz twice, and a
/// transcript can be replayed against the questions that produced it.
function plan(
    items: Item[],
    seed: Seed,
    weights: Weights,
    length: int,
    budget: Budget,
) -> Question[] throws never {
    served(session(items, seed, weights, length, budget)).map((turn) -> {
        question_of(turn)
    })
}

/// The `index`-th question of a sitting, or `null` past its end.
function question_at(
    items: Item[],
    seed: Seed,
    weights: Weights,
    length: int,
    budget: Budget,
    index: int,
) -> Question? throws never {
    plan(items, seed, weights, length, budget).at(index)
}

/// What the compiler actually says about a case: the codes and the messages,
/// as reported.
///
/// This is for showing a learner after they have answered. The quiz never
/// decides anything by it — the key already went through `verify` — but a
/// claim about a program is worth more beside the compiler's own words than
/// on its own.
class Reported {
    compiles: bool,
    codes: string[],
    messages: string[],
}

function report(case: Case) -> Reported throws never {
    match (compile(case.files)) {
        let failed: CompileFailed => {
            Reported {
                compiles: false,
                codes: codes_of(failed.diagnostics),
                messages: messages_of(failed.diagnostics),
            }
        },
        let threw: InitThrew => Reported { compiles: true, codes: [], messages: [threw.message] },
        reflect.Package => Reported { compiles: true, codes: [], messages: [] },
    }
}
