// Naive models: a plausible wrong intuition, expressed as a sparse set of
// overrides on rules. A model agrees with the spec wherever it says nothing,
// so it is never a second type system, only a list of the places one
// intuition parts company with BAML. The bank defines the models; the
// derivation replay in `relation.baml` consults them.

class Model {
    id: string,
    /// One paragraph for the grader, in the second person: what a learner
    /// running this model believes.
    description: string,
    /// For a rule that asserts a pair's relation: what the model believes
    /// about that pair, or `null` to defer to the spec for these types.
    believes: map<string, (Ty, Ty) -> Relation? throws never>,
    /// For a rule that transforms a relation: the model's transfer instead.
    transfers: map<string, (Knowledge) -> Knowledge throws never>,
}

function believed(model: Model?, step: Asserted) -> Relation? throws never {
    let m = model
        ?? return null;
    let opinion = m.believes.get(step.rule.name);
    if (opinion != null) {
        opinion(step.left, step.right)
    } else {
        null
    }
}

function transferred(
    model: Model?,
    step: Transferred,
    current: Knowledge,
) -> Knowledge? throws never {
    let m = model
        ?? return null;
    let opinion = m.transfers.get(step.rule.name);
    if (opinion != null) {
        opinion(current)
    } else {
        null
    }
}
