// The judgment a derivation carries, and how type constructors transform it.
//
// A generator never decides whether two types are related. It starts from a
// pair with a known relation and applies constructors whose effect on the
// relation is a spec rule; each rule is a transfer function here. When a
// transfer cannot say, the answer is `Undecided`, and an undecided derivation
// yields no case.

/// How `left` relates to `right`, as sets of values.
enum Relation {
    /// `left <: right`, strictly.
    Sub,
    /// `right <: left`, strictly.
    Super,
    /// The same set of values.
    Equiv,
    /// Neither is a subset of the other.
    Unrelated,
}

/// Nothing is known by construction. Distinct from `Relation` so a case can
/// never carry it.
class Undecided {
}

type Knowledge = Relation | Undecided;

/// One rule application and the knowledge it leaves.
class Step {
    rule: root.engine.Rule,
    after: Knowledge,
}

/// Two types and the derivation of what is known about their relation.
/// With no steps the sides are the same type and the relation is `Equiv`.
class Derivation {
    left: Ty,
    right: Ty,
    steps: Step[],
}

function start(ty: Ty) -> Derivation throws never {
    Derivation { left: ty, right: ty, steps: [] }
}

function knowledge(d: Derivation) -> Knowledge throws never {
    match (d.steps.at(d.steps.length() - 1)) {
        let last: Step => last.after,
        null => Relation.Equiv,
    }
}

/// `Sub` becomes `Super` and back; the rest are their own mirror.
function flip(k: Knowledge) -> Knowledge throws never {
    match (k) {
        Relation.Sub => Relation.Super,
        Relation.Super => Relation.Sub,
        Relation.Equiv => Relation.Equiv,
        Relation.Unrelated => Relation.Unrelated,
        Undecided => Undecided {  },
    }
}

/// A position that preserves the relation, such as a function's return type.
function covariant(k: Knowledge) -> Knowledge throws never {
    k
}

/// A position that reverses the relation, such as a function's parameter.
function contravariant(k: Knowledge) -> Knowledge throws never {
    flip(k)
}

/// A position that relates the wrapped types only when the arguments are the
/// same type: class generics, arrays, and maps.
function invariant(k: Knowledge) -> Knowledge throws never {
    match (k) {
        Relation.Sub => Relation.Unrelated,
        Relation.Super => Relation.Unrelated,
        Relation.Equiv => Relation.Equiv,
        Relation.Unrelated => Relation.Unrelated,
        Undecided => Undecided {  },
    }
}

/// A position that preserves an ordering but can absorb a difference, such
/// as a union member beside other members: `1 | int` and `2 | int` are the
/// same type although `1` and `2` are unrelated.
function absorbing(k: Knowledge) -> Knowledge throws never {
    match (k) {
        Relation.Sub => Relation.Sub,
        Relation.Super => Relation.Super,
        Relation.Equiv => Relation.Equiv,
        Relation.Unrelated => Undecided {  },
        Undecided => Undecided {  },
    }
}

/// Whether a value of `left` can flow into a slot of `right`.
function flows(k: Relation) -> bool throws never {
    match (k) {
        Relation.Sub => true,
        Relation.Equiv => true,
        Relation.Super => false,
        Relation.Unrelated => false,
    }
}
