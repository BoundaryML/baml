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

/// `k` as a relation, or `null` when nothing is known.
function decided(k: Knowledge) -> Relation? throws never {
    match (k) {
        let r: Relation => r,
        Undecided => null,
    }
}

/// A rule that states the relation of a pair of types directly.
class Asserted {
    rule: root.engine.Rule,
    left: Ty,
    right: Ty,
    relation: Relation,
}

/// A rule that wraps both sides of a pair in a constructor and transforms
/// what is known about them. `left` and `right` are the wrapped pair.
class Transferred {
    rule: root.engine.Rule,
    left: Ty,
    right: Ty,
    transfer: (Knowledge) -> Knowledge throws never,
}

type Step = Asserted | Transferred;

function step_rule(step: Step) -> root.engine.Rule throws never {
    match (step) {
        let a: Asserted => a.rule,
        let t: Transferred => t.rule,
    }
}

function step_left(step: Step) -> Ty throws never {
    match (step) {
        let a: Asserted => a.left,
        let t: Transferred => t.left,
    }
}

function step_right(step: Step) -> Ty throws never {
    match (step) {
        let a: Asserted => a.right,
        let t: Transferred => t.right,
    }
}

/// The derivation of what is known about a pair of types: one asserted
/// pair, then the constructors applied to both sides. The pair the
/// derivation is about is its last step's.
class Derivation {
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

/// The step whose pair `d` is about.
function last_step(d: Derivation) -> Step throws never {
    let fallback: Step = d.head;
    steps(d).at(-1) ?? fallback
}

function left_of(d: Derivation) -> Ty throws never {
    step_left(last_step(d))
}

function right_of(d: Derivation) -> Ty throws never {
    step_right(last_step(d))
}

/// A step and the knowledge after it.
class Judged {
    step: Step,
    after: Knowledge,
}

/// Every step of `d` with the knowledge after it, replayed under `model`
/// where it has an opinion and under the spec elsewhere. The spec's own
/// reading is `judgments(d, null)`.
function judgments(d: Derivation, model: Model?) -> Judged[] throws never {
    let first: Knowledge = believed(model, d.head) ?? d.head.relation;
    let start: Judged[] = [Judged { step: d.head, after: first }];
    d.rest.reduce(
        (acc, t) -> {
            let current = acc.at(-1)?.after ?? first;
            let after = transferred(model, t, current) ?? t.transfer(current);
            acc.concat([Judged { step: t, after }])
        },
        start,
    )
}

/// The knowledge after each step of `d`.
function trajectory(d: Derivation, model: Model?) -> Knowledge[] throws never {
    judgments(d, model).map((j) -> {
        j.after
    })
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

// ─── explaining derivations ──────────────────────────────────────────────────

/// Whether a transfer says the same thing about its pair whatever strict
/// relation went in.
///
/// Such a step SCREENS OFF everything below it: once it has been applied the
/// pair is related the one way regardless, so a learner can answer without
/// knowing which strict relation held, or that one held at all. Invariance is
/// the transfer we have that does this — `Box<S>` against `Box<T>` is
/// rejected for every strict pair, which is why a learner who knows only that
/// the two types are written differently gets it right.
function screens(transfer: (Knowledge) -> Knowledge throws never) -> bool throws never {
    let from_sub = transfer(Relation.Sub);
    from_sub == transfer(Relation.Super) && from_sub == transfer(Relation.Unrelated)
}

/// Which of `judged`'s steps the answer turns on.
///
/// Everything below the outermost screening step is inert, with one exception
/// that matters: an equivalence is carried through invariance unchanged, so a
/// screening step fed an equivalence screens nothing and the rule beneath it
/// is exactly what the question is about. `Box<bool>` against
/// `Box<true | false>` turns on knowing the two are one type; `Box<int>`
/// against `Box<int | string>` turns on nothing but invariance.
function bearing_of(judged: Judged[]) -> bool[] throws never {
    let cut = baml.iter.Range.new(1, judged.length()).reduce(
        (acc, i) -> {
            let into = judged[i - 1].after;
            match (judged[i].step) {
                let t: Transferred => {
                    if (screens(t.transfer) && into != Relation.Equiv) {
                        i
                    } else {
                        acc
                    }
                },
                Asserted => acc,
            }
        },
        0,
    );
    baml.iter.Range.new(0, judged.length())
        .map((i) -> {
            i >= cut
        })
        .collect()
}

/// The explanation of `d` under the spec: one claim per step, each rule
/// instantiated at the step's pair, and whether the answer turns on it.
function claims(d: Derivation) -> root.engine.Claim[] throws never {
    let judged = judgments(d, null);
    let bearing = bearing_of(judged);
    baml.iter.Range.new(0, judged.length())
        .map((i) -> {
            root.engine.Claim {
                rule: step_rule(judged[i].step),
                instance: instance_of(judged[i]),
                bearing: bearing[i],
            }
        })
        .collect()
}

/// A step's rule at its pair, in prose. A transferred step reads as a
/// consequence of the step before it.
function instance_of(j: Judged) -> string throws never {
    let sentence = relates(step_left(j.step), step_right(j.step), j.after);
    match (j.step) {
        Asserted => sentence,
        Transferred => `so ${sentence}`,
    }
}

/// `a` and `b` with `k` between them, in prose, the types in backticks.
function relates(a: Ty, b: Ty, k: Knowledge) -> string throws never {
    let l = `\`${render(a)}\``;
    let r = `\`${render(b)}\``;
    match (k) {
        Relation.Sub => `${l} is a strict subtype of ${r}`,
        Relation.Super => `${l} is a strict supertype of ${r}`,
        Relation.Equiv => `${l} and ${r} are the same type`,
        Relation.Unrelated => `neither of ${l} and ${r} is a subtype of the other`,
        Undecided => `how ${l} relates to ${r} does not follow`,
    }
}

// ─── transfers ───────────────────────────────────────────────────────────────

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
    Derivation { head: Asserted { rule, left, right, relation }, rest: [] }
}

/// Wrap both sides in a constructor whose effect on the relation is
/// `transfer`, as stated by `rule`; `left` and `right` are the wrapped pair.
function then(
    d: Derivation,
    rule: root.engine.Rule,
    left: Ty,
    right: Ty,
    transfer: (Knowledge) -> Knowledge throws never,
) -> Derivation throws never {
    Derivation { head: d.head, rest: d.rest.concat([Transferred { rule, left, right, transfer }]) }
}

/// `T[]` against `U[]`: invariant.
function in_array(d: Derivation, rule: root.engine.Rule) -> Derivation throws never {
    then(d, rule, Array.new(left_of(d)), Array.new(right_of(d)), invariant)
}

/// `map<string, T>` against `map<string, U>`: invariant.
function in_map_value(d: Derivation, rule: root.engine.Rule) -> Derivation throws never {
    then(d, rule, Map.new(left_of(d)), Map.new(right_of(d)), invariant)
}

/// `C<T>` against `C<U>` for a single-parameter class: invariant.
function in_class_arg(
    d: Derivation,
    decl: ClassDecl,
    rule: root.engine.Rule,
) -> Derivation throws never {
    then(d, rule, Class.new(decl, [left_of(d)]), Class.new(decl, [right_of(d)]), invariant)
}

/// `(T) -> R` against `(U) -> R`: contravariant.
function in_fn_param(d: Derivation, ret: Ty, rule: root.engine.Rule) -> Derivation throws never {
    then(
        d,
        rule,
        Fn.new([left_of(d)], [], ret, Never {  }),
        Fn.new([right_of(d)], [], ret, Never {  }),
        contravariant,
    )
}

/// `() -> T` against `() -> U`: covariant.
function in_fn_return(d: Derivation, rule: root.engine.Rule) -> Derivation throws never {
    then(
        d,
        rule,
        Fn.new([], [], left_of(d), Never {  }),
        Fn.new([], [], right_of(d), Never {  }),
        covariant,
    )
}

/// `() -> R throws T` against `() -> R throws U`: covariant.
function in_fn_error(d: Derivation, ret: Ty, rule: root.engine.Rule) -> Derivation throws never {
    then(d, rule, Fn.new([], [], ret, left_of(d)), Fn.new([], [], ret, right_of(d)), covariant)
}

/// `T | X` against `U | X`. The relation survives only when `X` is a leaf
/// that occurs in neither side, so it can absorb no difference between them;
/// otherwise the step is undecided and the derivation yields no case.
function in_union_with(
    d: Derivation,
    extra: Ty,
    rule: root.engine.Rule,
) -> Derivation throws never {
    let l = left_of(d);
    let r = right_of(d);
    let transfer = if (is_leaf(extra) && !mentions(l, extra) && !mentions(r, extra)) {
        absorbing
    } else {
        undecided
    };
    then(d, rule, Union.new([l, extra]), Union.new([r, extra]), transfer)
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
