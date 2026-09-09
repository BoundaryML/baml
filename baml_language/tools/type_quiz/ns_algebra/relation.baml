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

/// A rule that states the relation of a pair of types directly.
class Asserted {
    rule: root.engine.Rule,
    left: Ty,
    right: Ty,
    relation: Relation,
}

/// A rule that wraps both sides of a pair in a constructor and transforms
/// what is known about them.
class Transferred {
    rule: root.engine.Rule,
    transfer: (Knowledge) -> Knowledge throws never,
}

type Step = Asserted | Transferred;

function step_rule(step: Step) -> root.engine.Rule throws never {
    match (step) {
        let a: Asserted => a.rule,
        let t: Transferred => t.rule,
    }
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

/// The knowledge after each step, replayed under `model` where it has an
/// opinion and under the spec elsewhere. The spec's own trajectory is
/// `trajectory(d, null)`.
function trajectory(d: Derivation, model: Model?) -> Knowledge[] throws never {
    let states: Knowledge[] = [];
    let current: Knowledge = Relation.Equiv;
    for (let step in d.steps) {
        current
            = match (step) {
                let a: Asserted => believed(model, a) ?? a.relation,
                let t: Transferred => match (transferred(model, t, current)) {
                    let k: Knowledge => k,
                    null => t.transfer(current),
                },
            };
        let _ = states.push(current);
    }
    states
}

/// What the spec knows about `d`.
function knowledge(d: Derivation) -> Knowledge throws never {
    last_state(trajectory(d, null))
}

/// What a naive model would conclude about `d`.
function under(d: Derivation, model: Model) -> Knowledge throws never {
    last_state(trajectory(d, model))
}

function last_state(states: Knowledge[]) -> Knowledge throws never {
    match (states.at(states.length() - 1)) {
        let k: Knowledge => k,
        null => Relation.Equiv,
    }
}

/// How many times the trajectory turns a subtype into a supertype or back.
function flips(d: Derivation) -> int throws never {
    let count = 0;
    let previous: Knowledge = Relation.Equiv;
    for (let state in trajectory(d, null)) {
        let flipped = match (state) {
            Relation.Sub => previous == Relation.Super,
            Relation.Super => previous == Relation.Sub,
            Relation.Equiv => false,
            Relation.Unrelated => false,
            Undecided => false,
        };
        if (flipped) {
            count = count + 1;
        }
        previous = state;
    }
    count
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

/// `flip` on a decided relation.
function mirror(r: Relation) -> Relation throws never {
    match (r) {
        Relation.Sub => Relation.Super,
        Relation.Super => Relation.Sub,
        Relation.Equiv => Relation.Equiv,
        Relation.Unrelated => Relation.Unrelated,
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

// ─── building derivations ────────────────────────────────────────────────────

/// A pair whose relation a rule states directly.
function pair(
    left: Ty,
    right: Ty,
    relation: Relation,
    rule: root.engine.Rule,
) -> Derivation throws never {
    Derivation {
        left: left,
        right: right,
        steps: [Asserted { rule: rule, left: left, right: right, relation: relation }],
    }
}

/// Wrap both sides in a constructor whose effect on the relation is
/// `transfer`, as stated by `rule`.
function then(
    d: Derivation,
    rule: root.engine.Rule,
    left: Ty,
    right: Ty,
    transfer: (Knowledge) -> Knowledge throws never,
) -> Derivation throws never {
    Derivation {
        left: left,
        right: right,
        steps: d.steps.concat([Transferred { rule: rule, transfer: transfer }]),
    }
}

/// `T[]` against `U[]`: invariant.
function in_array(d: Derivation, rule: root.engine.Rule) -> Derivation throws never {
    then(d, rule, array(d.left), array(d.right), invariant)
}

/// `map<K, T>` against `map<K, U>`: invariant.
function in_map_value(d: Derivation, key: Ty, rule: root.engine.Rule) -> Derivation throws never {
    then(d, rule, map(key, d.left), map(key, d.right), invariant)
}

/// `C<T>` against `C<U>` for a single-parameter class: invariant.
function in_class_arg(
    d: Derivation,
    decl: ClassDecl,
    rule: root.engine.Rule,
) -> Derivation throws never {
    then(d, rule, class_ty(decl, [d.left]), class_ty(decl, [d.right]), invariant)
}

/// `(T) -> R` against `(U) -> R`: contravariant.
function in_fn_param(d: Derivation, ret: Ty, rule: root.engine.Rule) -> Derivation throws never {
    then(
        d,
        rule,
        fn([positional(d.left)], ret, never()),
        fn([positional(d.right)], ret, never()),
        contravariant,
    )
}

/// `() -> T` against `() -> U`: covariant.
function in_fn_return(d: Derivation, rule: root.engine.Rule) -> Derivation throws never {
    then(d, rule, fn([], d.left, never()), fn([], d.right, never()), covariant)
}

/// `() -> R throws T` against `() -> R throws U`: covariant.
function in_fn_error(d: Derivation, ret: Ty, rule: root.engine.Rule) -> Derivation throws never {
    then(d, rule, fn([], ret, d.left), fn([], ret, d.right), covariant)
}

/// `T | X` against `U | X`: absorbing.
function in_union_with(
    d: Derivation,
    extra: Ty,
    rule: root.engine.Rule,
) -> Derivation throws never {
    then(d, rule, union([d.left, extra]), union([d.right, extra]), absorbing)
}
