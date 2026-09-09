// Facts: seeded generators of a pair of types with a derived relation.
//
// Each fact is one spec rule applied to seeded types, or a context rule
// applied to another fact. A fact never decides a relation; it applies rules
// whose transfer functions live in the algebra, and the compiler checks the
// result through the items built on it.

/// A derivation and the seed that continues the stream.
class Derived {
    derivation: root.algebra.Derivation,
    next: root.engine.Seed,
}

/// A named fact generator. `name` must equal the generating function's name,
/// which the conformance suite checks by reflection so no fact can be
/// written and left out of `facts()`.
class Fact {
    name: string,
    generate: (root.engine.Seed) -> Derived throws never,
}

function run_fact(fact: Fact, seed: root.engine.Seed) -> Derived throws never {
    let generate = fact.generate;
    generate(seed)
}

// ─── seeded pools ────────────────────────────────────────────────────────────

/// The leaf types facts are built from: distinct concrete builtins, which
/// the spec says are pairwise unrelated.
function leaves() -> root.algebra.Ty[] throws never {
    [
        root.algebra.int(),
        root.algebra.string(),
        root.algebra.bool(),
        root.algebra.float(),
        root.algebra.bigint(),
    ]
}

class Leaves {
    tys: root.algebra.Ty[],
    next: root.engine.Seed,
}

/// `n` distinct leaves.
function pick_leaves(seed: root.engine.Seed, n: int) -> Leaves throws never {
    let remaining = leaves();
    let tys: root.algebra.Ty[] = [];
    let s = seed;
    while (tys.length() < n && remaining.length() > 0) {
        let d = root.engine.below(s, remaining.length());
        s = d.next;
        if let ty: root.algebra.Ty = remaining.remove_at(d.value) {
            let _ = tys.push(ty);
        }
    }
    Leaves { tys: tys, next: s }
}

class Literal {
    literal: root.algebra.Ty,
    concrete: root.algebra.Ty,
    next: root.engine.Seed,
}

/// A literal type and the concrete type it belongs to.
function pick_literal(seed: root.engine.Seed) -> Literal throws never {
    let d = root.engine.below(seed, 3);
    let n = root.engine.below(d.next, 100);
    match (d.value) {
        0 => {
            Literal {
                literal: root.algebra.lit(n.value),
                concrete: root.algebra.int(),
                next: n.next,
            }
        },
        1 => {
            Literal {
                literal: root.algebra.lit("sheep"),
                concrete: root.algebra.string(),
                next: n.next,
            }
        },
        _ => {
            Literal {
                literal: root.algebra.lit(n.value % 2 == 0),
                concrete: root.algebra.bool(),
                next: n.next,
            }
        },
    }
}

class NamedDecl {
    name: string,
    next: root.engine.Seed,
}

function pick_type_name(seed: root.engine.Seed) -> NamedDecl throws never {
    let picked = pick_distinct(seed, type_names(), 1);
    NamedDecl { name: picked.names[0], next: picked.next }
}

/// `class Name<T> { value: T }` with a seeded name.
function box_decl(seed: root.engine.Seed) -> BoxDecl throws never {
    let n = pick_type_name(seed);
    BoxDecl {
        decl: root.algebra.ClassDecl {
            name: n.name,
            params: ["T"],
            fields: [root.algebra.FieldDecl { name: "value", ty: root.algebra.var("T") }],
        },
        next: n.next,
    }
}

class BoxDecl {
    decl: root.algebra.ClassDecl,
    next: root.engine.Seed,
}

function variant_names() -> string[] throws never {
    ["Left", "Right", "Up", "Down", "North", "South", "Red", "Green"]
}

class EnumPick {
    decl: root.algebra.EnumDecl,
    next: root.engine.Seed,
}

/// An enum with a seeded name and two or three seeded variants.
function pick_enum(seed: root.engine.Seed) -> EnumPick throws never {
    let n = pick_type_name(seed);
    let count = root.engine.below(n.next, 2);
    let variants = pick_distinct(count.next, variant_names(), 2 + count.value);
    EnumPick {
        decl: root.algebra.EnumDecl { name: n.name, variants: variants.names },
        next: variants.next,
    }
}

// ─── pair-introducing facts ──────────────────────────────────────────────────

function derived(d: root.algebra.Derivation, next: root.engine.Seed) -> Derived throws never {
    Derived { derivation: d, next: next }
}

function never_bottom(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 1);
    derived(
        root.algebra.pair(
            root.algebra.never(),
            l.tys[0],
            root.algebra.Relation.Sub,
            never_bottom_rule(),
        ),
        l.next,
    )
}

function unknown_top(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 1);
    derived(
        root.algebra.pair(
            l.tys[0],
            root.algebra.unknown(),
            root.algebra.Relation.Sub,
            unknown_top_rule(),
        ),
        l.next,
    )
}

function identity(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 1);
    derived(root.algebra.pair(l.tys[0], l.tys[0], root.algebra.Relation.Equiv, identity_rule()), l.next)
}

function union_member(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 2);
    derived(
        root.algebra.pair(
            l.tys[0],
            root.algebra.union([l.tys[0], l.tys[1]]),
            root.algebra.Relation.Sub,
            union_member_rule(),
        ),
        l.next,
    )
}

function union_subset(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 3);
    derived(
        root.algebra.pair(
            root.algebra.union([l.tys[0], l.tys[1]]),
            root.algebra.union([l.tys[0], l.tys[1], l.tys[2]]),
            root.algebra.Relation.Sub,
            union_subset_rule(),
        ),
        l.next,
    )
}

function literal_subtype(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_literal(seed);
    derived(
        root.algebra.pair(
            l.literal,
            l.concrete,
            root.algebra.Relation.Sub,
            literal_subtype_of_concrete(),
        ),
        l.next,
    )
}

function concrete_atomic(seed: root.engine.Seed) -> Derived throws never {
    // Half the seeds pit the two numeric types against each other, so the
    // TypeScript intuition that they are one `number` is exercised often.
    let choice = root.engine.below(seed, 2);
    let l = if (choice.value == 0) {
        let order = root.engine.below(choice.next, 2);
        let tys = if (order.value == 0) {
            [root.algebra.int(), root.algebra.float()]
        } else {
            [root.algebra.float(), root.algebra.int()]
        };
        Leaves { tys: tys, next: order.next }
    } else {
        pick_leaves(choice.next, 2)
    };
    derived(
        root.algebra.pair(
            l.tys[0],
            l.tys[1],
            root.algebra.Relation.Unrelated,
            concrete_types_atomic(),
        ),
        l.next,
    )
}

function bool_true_false(seed: root.engine.Seed) -> Derived throws never {
    let order = root.engine.below(seed, 2);
    let members = if (order.value == 0) {
        [root.algebra.lit(true), root.algebra.lit(false)]
    } else {
        [root.algebra.lit(false), root.algebra.lit(true)]
    };
    derived(
        root.algebra.pair(
            root.algebra.union(members),
            root.algebra.bool(),
            root.algebra.Relation.Equiv,
            bool_is_true_or_false(),
        ),
        order.next,
    )
}

function enum_variants(seed: root.engine.Seed) -> Derived throws never {
    let e = pick_enum(seed);
    let members = e.decl.variants.map((v) -> {
        root.algebra.enum_lit(e.decl, v)
    });
    derived(
        root.algebra.pair(
            root.algebra.union(members),
            root.algebra.enum_ty(e.decl),
            root.algebra.Relation.Equiv,
            enum_is_union_of_variants(),
        ),
        e.next,
    )
}

function literal_absorbed_fact(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_literal(seed);
    let order = root.engine.below(l.next, 2);
    let members = if (order.value == 0) {
        [l.literal, l.concrete]
    } else {
        [l.concrete, l.literal]
    };
    derived(
        root.algebra.pair(
            root.algebra.union(members),
            l.concrete,
            root.algebra.Relation.Equiv,
            literal_absorbed(),
        ),
        order.next,
    )
}

function unknown_absorbs_fact(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 1);
    derived(
        root.algebra.pair(
            root.algebra.union([l.tys[0], root.algebra.unknown()]),
            root.algebra.unknown(),
            root.algebra.Relation.Equiv,
            unknown_absorbs(),
        ),
        l.next,
    )
}

function never_omitted(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 1);
    derived(
        root.algebra.pair(
            root.algebra.union([l.tys[0], root.algebra.never()]),
            l.tys[0],
            root.algebra.Relation.Equiv,
            never_omitted_from_unions(),
        ),
        l.next,
    )
}

function union_commutes(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 2);
    derived(
        root.algebra.pair(
            root.algebra.union([l.tys[0], l.tys[1]]),
            root.algebra.union([l.tys[1], l.tys[0]]),
            root.algebra.Relation.Equiv,
            union_commutative(),
        ),
        l.next,
    )
}

function union_associates(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 3);
    derived(
        root.algebra.pair(
            root.algebra.union([l.tys[0], root.algebra.union([l.tys[1], l.tys[2]])]),
            root.algebra.union([l.tys[0], l.tys[1], l.tys[2]]),
            root.algebra.Relation.Equiv,
            union_associative(),
        ),
        l.next,
    )
}

function alias_is_transparent(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 1);
    let n = pick_type_name(l.next);
    let decl = root.algebra.AliasDecl { name: n.name, body: l.tys[0] };
    derived(
        root.algebra.pair(
            root.algebra.Alias { decl: decl },
            l.tys[0],
            root.algebra.Relation.Equiv,
            alias_transparent(),
        ),
        n.next,
    )
}

/// `type A = leaf | A[]` under a seeded name.
function list_alias(name: string, leaf: root.algebra.Ty) -> root.algebra.AliasDecl throws never {
    root.algebra.recursive_alias(name, (self) -> {
        root.algebra.union([leaf, root.algebra.array(self)])
    })
}

function equirecursive(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 1);
    let names = pick_distinct(l.next, type_names(), 2);
    derived(
        root.algebra.pair(
            root.algebra.Alias { decl: list_alias(names.names[0], l.tys[0]) },
            root.algebra.Alias { decl: list_alias(names.names[1], l.tys[0]) },
            root.algebra.Relation.Equiv,
            equirecursive_identity(),
        ),
        names.next,
    )
}

function unfolds_once(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 1);
    let n = pick_type_name(l.next);
    let decl = list_alias(n.name, l.tys[0]);
    derived(
        root.algebra.pair(
            root.algebra.Alias { decl: decl },
            root.algebra.union([l.tys[0], root.algebra.array(root.algebra.Alias { decl: decl })]),
            root.algebra.Relation.Equiv,
            partial_unfolding(),
        ),
        n.next,
    )
}

function unguarded_member_is_vacuous(seed: root.engine.Seed) -> Derived throws never {
    let names = pick_distinct(seed, type_names(), 2);
    let with_member = root.algebra.recursive_alias(names.names[0], (self) -> {
        root.algebra.union([self, root.algebra.array(self)])
    });
    let plain = root.algebra.recursive_alias(names.names[1], (self) -> {
        root.algebra.array(self)
    });
    derived(
        root.algebra.pair(
            root.algebra.Alias { decl: with_member },
            root.algebra.Alias { decl: plain },
            root.algebra.Relation.Equiv,
            unguarded_member_vacuous(),
        ),
        names.next,
    )
}

function optional_superset(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 3);
    let names = pick_distinct(l.next, binding_names(), 2);
    let more = root.algebra.fn(
        [
            root.algebra.named(names.names[0], l.tys[0], false),
            root.algebra.named(names.names[1], l.tys[1], true),
        ],
        l.tys[2],
        root.algebra.never(),
    );
    let fewer = root.algebra.fn(
        [root.algebra.named(names.names[0], l.tys[0], false)],
        l.tys[2],
        root.algebra.never(),
    );
    derived(root.algebra.pair(more, fewer, root.algebra.Relation.Sub, fn_optional_superset()), names.next)
}

function positional_count(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 2);
    let one = root.algebra.fn([root.algebra.positional(l.tys[0])], l.tys[1], root.algebra.never());
    let two = root.algebra.fn(
        [root.algebra.positional(l.tys[0]), root.algebra.positional(l.tys[0])],
        l.tys[1],
        root.algebra.never(),
    );
    derived(root.algebra.pair(one, two, root.algebra.Relation.Unrelated, fn_positional_count()), l.next)
}

// ─── context facts ───────────────────────────────────────────────────────────

/// A seeded strict-subtype pair from one of the pair-introducing facts, so
/// context facts vary in what they wrap.
function sub_pair(seed: root.engine.Seed) -> Derived throws never {
    let d = root.engine.below(seed, 5);
    match (d.value) {
        0 => never_bottom(d.next),
        1 => unknown_top(d.next),
        2 => union_member(d.next),
        3 => union_subset(d.next),
        _ => literal_subtype(d.next),
    }
}

function class_arg_invariant(seed: root.engine.Seed) -> Derived throws never {
    let inner = sub_pair(seed);
    let b = box_decl(inner.next);
    derived(root.algebra.in_class_arg(inner.derivation, b.decl, generics_invariant()), b.next)
}

function array_invariant(seed: root.engine.Seed) -> Derived throws never {
    let inner = sub_pair(seed);
    derived(root.algebra.in_array(inner.derivation, generics_invariant()), inner.next)
}

function map_value_invariant(seed: root.engine.Seed) -> Derived throws never {
    let inner = sub_pair(seed);
    derived(
        root.algebra.in_map_value(inner.derivation, root.algebra.string(), generics_invariant()),
        inner.next,
    )
}

function param_contravariant(seed: root.engine.Seed) -> Derived throws never {
    let inner = sub_pair(seed);
    let r = pick_leaves(inner.next, 1);
    derived(root.algebra.in_fn_param(inner.derivation, r.tys[0], fn_param_contravariant()), r.next)
}

function return_covariant(seed: root.engine.Seed) -> Derived throws never {
    let inner = sub_pair(seed);
    derived(root.algebra.in_fn_return(inner.derivation, fn_return_covariant()), inner.next)
}

function error_covariant(seed: root.engine.Seed) -> Derived throws never {
    let inner = sub_pair(seed);
    let r = pick_leaves(inner.next, 1);
    derived(root.algebra.in_fn_error(inner.derivation, r.tys[0], fn_error_covariant()), r.next)
}

function box_of_bool(seed: root.engine.Seed) -> Derived throws never {
    let inner = bool_true_false(seed);
    let b = box_decl(inner.next);
    derived(root.algebra.in_class_arg(inner.derivation, b.decl, generics_invariant()), b.next)
}

function double_contravariance(seed: root.engine.Seed) -> Derived throws never {
    let inner = sub_pair(seed);
    let r = pick_leaves(inner.next, 1);
    let once = root.algebra.in_fn_param(inner.derivation, r.tys[0], fn_param_contravariant());
    derived(root.algebra.in_fn_param(once, r.tys[0], fn_param_contravariant()), r.next)
}

function array_of_unfolded_alias(seed: root.engine.Seed) -> Derived throws never {
    let inner = unfolds_once(seed);
    derived(root.algebra.in_array(inner.derivation, generics_invariant()), inner.next)
}

function bottom_beside_member(seed: root.engine.Seed) -> Derived throws never {
    let inner = never_bottom(seed);
    let extra = pick_leaves(inner.next, 2);
    let other = if (extra.tys[0] == inner.derivation.right) {
        extra.tys[1]
    } else {
        extra.tys[0]
    };
    derived(root.algebra.in_union_with(inner.derivation, other, union_member_rule()), extra.next)
}

// ─── registry ────────────────────────────────────────────────────────────────

function facts() -> Fact[] throws never {
    [
        Fact { name: "never_bottom", generate: never_bottom },
        Fact { name: "unknown_top", generate: unknown_top },
        Fact { name: "identity", generate: identity },
        Fact { name: "union_member", generate: union_member },
        Fact { name: "union_subset", generate: union_subset },
        Fact { name: "literal_subtype", generate: literal_subtype },
        Fact { name: "concrete_atomic", generate: concrete_atomic },
        Fact { name: "bool_true_false", generate: bool_true_false },
        Fact { name: "enum_variants", generate: enum_variants },
        Fact { name: "literal_absorbed_fact", generate: literal_absorbed_fact },
        Fact { name: "unknown_absorbs_fact", generate: unknown_absorbs_fact },
        Fact { name: "never_omitted", generate: never_omitted },
        Fact { name: "union_commutes", generate: union_commutes },
        Fact { name: "union_associates", generate: union_associates },
        Fact { name: "alias_is_transparent", generate: alias_is_transparent },
        Fact { name: "equirecursive", generate: equirecursive },
        Fact { name: "unfolds_once", generate: unfolds_once },
        Fact { name: "unguarded_member_is_vacuous", generate: unguarded_member_is_vacuous },
        Fact { name: "optional_superset", generate: optional_superset },
        Fact { name: "positional_count", generate: positional_count },
        Fact { name: "sub_pair", generate: sub_pair },
        Fact { name: "class_arg_invariant", generate: class_arg_invariant },
        Fact { name: "array_invariant", generate: array_invariant },
        Fact { name: "map_value_invariant", generate: map_value_invariant },
        Fact { name: "param_contravariant", generate: param_contravariant },
        Fact { name: "return_covariant", generate: return_covariant },
        Fact { name: "error_covariant", generate: error_covariant },
        Fact { name: "box_of_bool", generate: box_of_bool },
        Fact { name: "double_contravariance", generate: double_contravariance },
        Fact { name: "array_of_unfolded_alias", generate: array_of_unfolded_alias },
        Fact { name: "bottom_beside_member", generate: bottom_beside_member },
    ]
}
