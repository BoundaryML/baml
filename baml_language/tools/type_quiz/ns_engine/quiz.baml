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

/// A served turn as a question. A sitting planned from a seed alone has no
/// model of its learner and so no rate to put choices at: it asks for the
/// verdict on the one program, which is what `plan` is for.
function question_of(turn: Served) -> Question throws never {
    Question {
        item: turn.chosen.item,
        seed: turn.chosen.seed.state,
        posed: turn.chosen.candidate.case,
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

// ─── how a case is put ───────────────────────────────────────────────────────

/// A candidate as it is put to a learner: what they see and are asked, and
/// the naive models that this particular question traps.
///
/// The two travel together because they are not independent: a model that a
/// program traps on its own need not be trapped by the choice between that
/// program and its foil, and evidence about the learner would otherwise be
/// credited to a model that was never in a position to mislead them.
class Put {
    posed: Posed,
    traps: string[],
}

/// The stream a case is put from: a child of the stream the case itself was
/// drawn from, so how a case is put is not decided by the bits that decided
/// what it says.
function pose_seed(state: int) -> Seed throws never {
    seed_for("pose", state)
}

/// Whether the case drawn at `seed` is to be put as a choice between two
/// programs, if it has two to offer.
///
/// A function of the case's own stream, so the same case is always put the
/// same way and a sitting can be worked out again from what a learner said
/// and nothing else.
function choosing(knobs: Knobs, seed: Seed) -> bool throws never {
    unit(split(seed).left) < knobs.contrast
}

/// How `candidate` is put at `seed`.
///
/// A candidate whose item offers a foil is put as a choice between the two
/// programs `knobs.contrast` of the time and as a verdict on the one program
/// the rest; a candidate without one is always a verdict. Which of the two
/// is shown first is an even draw, so a learner reading nothing but position
/// is right half the time and no more.
///
/// The foil arrives as a function rather than a value because building one
/// means rendering a second whole program, and the draw above decides
/// whether it is wanted: a sitting draws candidates by the handful and puts
/// one of them.
function put(
    candidate: Candidate,
    foil: () -> Foil? throws never,
    knobs: Knobs,
    seed: Seed,
) -> Put throws never {
    let alone = Put { posed: candidate.case, traps: candidate.served.traps };
    if (!choosing(knobs, seed)) {
        return alone;
    }
    let found = foil()
        ?? return alone;
    let order = below(split(seed).right, 2);
    let choice = contrast(candidate.case, found, order.value == 0)
        ?? return alone;
    Put { posed: choice, traps: choice.traps }
}

/// How a candidate already in hand is put at `seed`.
///
/// Everything `put` needs beyond the candidate follows from the item and the
/// seed it was drawn from, so a caller holding those three needs nothing
/// else. Selection holds the candidate it drew, and generating it a second
/// time to put it is the most expensive thing a step would otherwise do
/// twice; `put_at`, whose caller has only an id and a seed, generates it
/// because it has to.
function put_drawn(knobs: Knobs, item: Item, seed: Seed, candidate: Candidate) -> Put throws never {
    put(
        candidate,
        () -> {
            foil_of(item, seed)
        },
        knobs,
        pose_seed(seed.state),
    )
}

/// `case` and its `foil` as a choice, or `null` when the two do not in fact
/// disagree: a foil the compiler answers the same way poses no question.
function contrast(case: Case, foil: Foil, compiles_first: bool) -> Choice? throws never {
    let against = foil.case;
    match (verdict_kind(case.key)) {
        VerdictKind.Compiles => {
            if (verdict_kind(against.key) == VerdictKind.Rejected) {
                Choice {
                    compiles: case,
                    rejected: against,
                    difference: foil.difference,
                    traps: foil.traps,
                    compiles_first,
                }
            } else {
                null
            }
        },
        VerdictKind.Rejected => {
            if (verdict_kind(against.key) == VerdictKind.Compiles) {
                Choice {
                    compiles: against,
                    rejected: case,
                    difference: foil.difference,
                    traps: foil.traps,
                    compiles_first,
                }
            } else {
                null
            }
        },
    }
}

// ─── what a learner is shown once they have answered ─────────────────────────

/// One program of an answered question, as the reveal shows it.
class Verdicted {
    files: map<string, string>,
    compiles: bool,
    /// What the compiler says about it, in its own words.
    reported: Reported,
    /// What this program turns on that the other does not. Empty when there
    /// was only one, which shares everything with itself.
    claims: Claim[],
}

/// An answered question laid out for the learner: every program with what
/// the compiler makes of it, what they have in common, and what tells them
/// apart.
class Revealed {
    /// In the order they were shown.
    programs: Verdicted[],
    /// The claims every program shares.
    shared: Claim[],
    /// What differs between the programs; empty when only one was shown.
    difference: string,
}

/// A program's files as the one text a reader is shown.
function source_of(files: map<string, string>) -> string throws never {
    files.values().join("\n")
}

/// The claims a reason given for `said` has to reach.
///
/// Everything the programs shown have in common, and then what is true of
/// the one the learner says the compiler rejects. That second part is the
/// whole of what a rejection turns on, and it is exactly what a learner
/// giving a reason was asked to explain — so the rubric follows the question
/// rather than being assembled beside it, and a reason cannot be marked
/// against claims that decide nothing it was asked about. Someone who
/// committed to nothing was asked for nothing, and has no rubric.
///
/// With one program the claims are all shared, and its own are empty, so the
/// same expression covers it: there is no case here for one program and
/// another for two.
function rubric(revealed: Revealed, said: Said?) -> Claim[] throws never {
    let at = match (said) {
        null => {
            return [];
        },
        VerdictKind => 0,
        let chose: Chose => {
            if (chose.second) {
                0
            } else {
                1
            }
        },
    };
    revealed.shared.concat(revealed.programs.at(at)?.claims ?? [])
}

function reveal(posed: Posed) -> Revealed throws never {
    let cases = cases_of(posed);
    let shared = shared_claims(
        cases.map((case) -> {
            case.trace
        }),
    );
    Revealed {
        programs: cases.map((case) -> {
            Verdicted {
                files: case.files,
                compiles: verdict_kind(case.key) == VerdictKind.Compiles,
                reported: report(case),
                claims: case.trace.slice(shared.length(), case.trace.length()),
            }
        }),
        shared,
        difference: match (posed) {
            Case => "",
            let choice: Choice => choice.difference,
        },
    }
}
