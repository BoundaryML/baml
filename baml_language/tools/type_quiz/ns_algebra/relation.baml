// The judgment a derivation carries, and how type constructors transform it.
//
// A generator never decides whether two types are related. It starts from a
// pair whose relation one spec rule states and applies constructors whose
// effect on the relation is another spec rule; each rule is a transfer
// function here. When a transfer cannot say, the answer is `Undecided`, and
// an undecided derivation yields no case.

/// How `left` relates to `right`, as sets of values. `Sub` and `Super` are
/// strict: the sets differ.
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

/// Two types and the derivation of what is known about their relation: one
/// asserted pair, then the constructors applied to both sides.
class Derivation {
    left: Ty,
    right: Ty,
    head: Asserted,
    rest: Transferred[],
}

/// The steps of `d`, in order.
function steps(d: Derivation) -> Step[] throws never {
    let head: Step[] = [d.head];
    // Arrays are invariant, so `Transferred[]` is widened member by member.
    head.concat(
        d.rest.map((t) -> {
            let s: Step = t;
            s
        }),
    )
}

/// The knowledge after each step, replayed under `model` where it has an
/// opinion and under the spec elsewhere. The spec's own trajectory is
/// `trajectory(d, null)`.
function trajectory(d: Derivation, model: Model?) -> Knowledge[] throws never {
    let first: Knowledge = believed(model, d.head) ?? d.head.relation;
    d.rest.reduce(
        (states, t) -> {
            let current = states.at(-1) ?? first;
            let after = transferred(model, t, current) ?? t.transfer(current);
            states.concat([after])
        },
        [first],
    )
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
    states.at(-1) ?? Relation.Equiv
}

/// How many times the trajectory turns a subtype into a supertype or back.
function flips(d: Derivation) -> int throws never {
    let states = trajectory(d, null);
    baml.iter.Range.new(1, states.length()).reduce(
        (count, i) -> {
            let flipped = match (states[i]) {
                Relation.Sub => states[i - 1] == Relation.Super,
                Relation.Super => states[i - 1] == Relation.Sub,
                Relation.Equiv | Relation.Unrelated | Undecided => false,
            };
            if (flipped) {
                count + 1
            } else {
                count
            }
        },
        0,
    )
}

/// `Sub` becomes `Super` and back; the rest are their own mirror.
function mirror(r: Relation) -> Relation throws never {
    match (r) {
        Relation.Sub => Relation.Super,
        Relation.Super => Relation.Sub,
        Relation.Equiv => Relation.Equiv,
        Relation.Unrelated => Relation.Unrelated,
    }
}

/// `mirror` on knowledge; `Undecided` stays undecided.
function flip(k: Knowledge) -> Knowledge throws never {
    match (k) {
        let r: Relation => mirror(r),
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
        Relation.Sub | Relation.Super | Relation.Unrelated => Relation.Unrelated,
        Relation.Equiv => Relation.Equiv,
        Undecided => Undecided {  },
    }
}

/// A union member beside a member that is a leaf type unrelated to either
/// side: the relation is preserved, because the extra member can absorb
/// nothing. An unrelated pair stays undecided (`1 | int` and `2 | int` are
/// the same type). Callers must establish the disjointness; see
/// `in_union_with`.
function absorbing(k: Knowledge) -> Knowledge throws never {
    match (k) {
        Relation.Sub => Relation.Sub,
        Relation.Super => Relation.Super,
        Relation.Equiv => Relation.Equiv,
        Relation.Unrelated | Undecided => Undecided {  },
    }
}

/// A transfer that knows nothing.
function undecided(k: Knowledge) -> Knowledge throws never {
    Undecided {  }
}

/// Whether a value of `left` can flow into a slot of `right`.
function flows(k: Relation) -> bool throws never {
    match (k) {
        Relation.Sub | Relation.Equiv => true,
        Relation.Super | Relation.Unrelated => false,
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
        head: Asserted { rule: rule, left: left, right: right, relation: relation },
        rest: [],
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
        head: d.head,
        rest: d.rest.concat([Transferred { rule: rule, transfer: transfer }]),
    }
}

/// `T[]` against `U[]`: invariant.
function in_array(d: Derivation, rule: root.engine.Rule) -> Derivation throws never {
    then(d, rule, Array.new(d.left), Array.new(d.right), invariant)
}

/// `map<string, T>` against `map<string, U>`: invariant.
function in_map_value(d: Derivation, rule: root.engine.Rule) -> Derivation throws never {
    then(d, rule, Map.new(d.left), Map.new(d.right), invariant)
}

/// `C<T>` against `C<U>` for a single-parameter class: invariant.
function in_class_arg(
    d: Derivation,
    decl: ClassDecl,
    rule: root.engine.Rule,
) -> Derivation throws never {
    then(d, rule, Class.new(decl, [d.left]), Class.new(decl, [d.right]), invariant)
}

/// `(T) -> R` against `(U) -> R`: contravariant.
function in_fn_param(d: Derivation, ret: Ty, rule: root.engine.Rule) -> Derivation throws never {
    then(
        d,
        rule,
        Fn.new([d.left], [], ret, Never {  }),
        Fn.new([d.right], [], ret, Never {  }),
        contravariant,
    )
}

/// `() -> T` against `() -> U`: covariant.
function in_fn_return(d: Derivation, rule: root.engine.Rule) -> Derivation throws never {
    then(d, rule, Fn.new([], [], d.left, Never {  }), Fn.new([], [], d.right, Never {  }), covariant)
}

/// `() -> R throws T` against `() -> R throws U`: covariant.
function in_fn_error(d: Derivation, ret: Ty, rule: root.engine.Rule) -> Derivation throws never {
    then(d, rule, Fn.new([], [], ret, d.left), Fn.new([], [], ret, d.right), covariant)
}

/// `T | X` against `U | X`. The relation survives only when `X` is a leaf
/// that occurs in neither side, so it can absorb no difference between them;
/// otherwise the step is undecided and the derivation yields no case.
function in_union_with(
    d: Derivation,
    extra: Ty,
    rule: root.engine.Rule,
) -> Derivation throws never {
    let transfer = if (is_leaf(extra) && !mentions(d.left, extra) && !mentions(d.right, extra)) {
        absorbing
    } else {
        undecided
    };
    then(d, rule, Union.new([d.left, extra]), Union.new([d.right, extra]), transfer)
}

/// A builtin concrete type, which the spec makes unrelated to every other.
function is_leaf(ty: Ty) -> bool throws never {
    ty is Prim
}

/// Whether `leaf` is `ty` or one of `ty`'s union members.
function mentions(ty: Ty, leaf: Ty) -> bool throws never {
    match (ty) {
        let u: Union => {
            u.members().some((m) -> {
                mentions(m, leaf)
            })
        },
        Prim | Never | Top | Lit | EnumLit | Array | Map | Fn | Class | Enum | Iface | Alias | Var => {
            ty == leaf
        },
    }
}
