// Facts: seeded generators of a pair of types with a derived relation.
//
// Each fact is one spec rule applied to seeded types, or a context rule
// applied to another fact. A fact never decides a relation; it applies rules
// whose transfer functions live in the algebra, and the compiler checks the
// result through the items built on it.

/// A derivation drawn from a seed.
type Derived = root.engine.Rolled<root.algebra.Derivation>;

/// A named fact generator. `name` must equal the generating function's name,
/// which the conformance suite checks by reflection so no fact can be
/// written and left out of `facts()`.
class Fact {
    name: string,
    generate: (root.engine.Seed) -> Derived throws never,
}

// ─── seeded pools ────────────────────────────────────────────────────────────

/// The leaf types facts are built from: distinct concrete builtins, which
/// the spec says are pairwise unrelated.
function leaves() -> root.algebra.Ty[] throws never {
    [
        root.algebra.Prim.int(),
        root.algebra.Prim.string(),
        root.algebra.Prim.bool(),
        root.algebra.Prim.float(),
        root.algebra.Prim.bigint(),
    ]
}

/// `n` distinct leaves; the pool holds five.
function pick_leaves(
    seed: root.engine.Seed,
    n: int,
) -> root.engine.Rolled<root.algebra.Ty[]> throws never {
    root.engine.pick_distinct(seed, leaves(), n)
}

/// A literal type and the concrete type it belongs to.
class LiteralPair {
    literal: root.algebra.Ty,
    concrete: root.algebra.Ty,
}

function pick_literal(seed: root.engine.Seed) -> root.engine.Rolled<LiteralPair> throws never {
    let kind = root.engine.below(seed, 3);
    let n = root.engine.below(kind.next, 100);
    let pair = match (kind.value) {
        0 => {
            LiteralPair {
                literal: root.algebra.Lit.new(n.value),
                concrete: root.algebra.Prim.int(),
            }
        },
        1 => {
            LiteralPair {
                literal: root.algebra.Lit.new("sheep"),
                concrete: root.algebra.Prim.string(),
            }
        },
        _ => {
            LiteralPair {
                literal: root.algebra.Lit.new(n.value % 2 == 0),
                concrete: root.algebra.Prim.bool(),
            }
        },
    };
    root.engine.Rolled<LiteralPair> { value: pair, next: n.next }
}

function pick_type_name(seed: root.engine.Seed) -> root.engine.Rolled<string> throws never {
    let picked = root.engine.pick_distinct(seed, type_names(), 1);
    root.engine.Rolled<string> { value: picked.value[0], next: picked.next }
}

/// `class Name<T> { value: T }` with a seeded name.
function box_decl(
    seed: root.engine.Seed,
) -> root.engine.Rolled<root.algebra.ClassDecl> throws never {
    let n = pick_type_name(seed);
    root.engine.Rolled<root.algebra.ClassDecl> {
        value: root.algebra.ClassDecl {
            name: n.value,
            params: ["T"],
            fields: [root.algebra.FieldDecl { name: "value", ty: root.algebra.Var.new("T") }],
        },
        next: n.next,
    }
}

function variant_names() -> string[] throws never {
    ["Left", "Right", "Up", "Down", "North", "South", "Red", "Green"]
}

/// An enum with a seeded name and two or three seeded variants.
function pick_enum(
    seed: root.engine.Seed,
) -> root.engine.Rolled<root.algebra.EnumDecl> throws never {
    let n = pick_type_name(seed);
    let count = root.engine.below(n.next, 2);
    let variants = root.engine.pick_distinct(count.next, variant_names(), 2 + count.value);
    root.engine.Rolled<root.algebra.EnumDecl> {
        value: root.algebra.EnumDecl { name: n.value, variants: variants.value },
        next: variants.next,
    }
}

// ─── pair-introducing facts ──────────────────────────────────────────────────

function derived(d: root.algebra.Derivation, next: root.engine.Seed) -> Derived throws never {
    root.engine.Rolled<root.algebra.Derivation> { value: d, next: next }
}

function never_bottom(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 1);
    derived(
        root.algebra.pair(
            root.algebra.Never {  },
            l.value[0],
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
            l.value[0],
            root.algebra.Top {  },
            root.algebra.Relation.Sub,
            unknown_top_rule(),
        ),
        l.next,
    )
}

function identity(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 1);
    derived(
        root.algebra.pair(l.value[0], l.value[0], root.algebra.Relation.Equiv, identity_rule()),
        l.next,
    )
}

function union_member(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 2);
    derived(
        root.algebra.pair(
            l.value[0],
            root.algebra.Union.new(l.value),
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
            root.algebra.Union.new(l.value.slice(0, 2)),
            root.algebra.Union.new(l.value),
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
            l.value.literal,
            l.value.concrete,
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
        let tys: root.algebra.Ty[] = if (order.value == 0) {
            [root.algebra.Prim.int(), root.algebra.Prim.float()]
        } else {
            [root.algebra.Prim.float(), root.algebra.Prim.int()]
        };
        root.engine.Rolled<root.algebra.Ty[]> { value: tys, next: order.next }
    } else {
        pick_leaves(choice.next, 2)
    };
    derived(
        root.algebra.pair(
            l.value[0],
            l.value[1],
            root.algebra.Relation.Unrelated,
            concrete_types_atomic(),
        ),
        l.next,
    )
}

function bool_true_false(seed: root.engine.Seed) -> Derived throws never {
    let order = root.engine.below(seed, 2);
    let members: root.algebra.Ty[] = if (order.value == 0) {
        [root.algebra.Lit.new(true), root.algebra.Lit.new(false)]
    } else {
        [root.algebra.Lit.new(false), root.algebra.Lit.new(true)]
    };
    derived(
        root.algebra.pair(
            root.algebra.Union.new(members),
            root.algebra.Prim.bool(),
            root.algebra.Relation.Equiv,
            bool_is_true_or_false(),
        ),
        order.next,
    )
}

function enum_variants(seed: root.engine.Seed) -> Derived throws never {
    let e = pick_enum(seed);
    // Arrays are invariant, so the variants are widened to `Ty` as they are made.
    let members = e.value.variants.map((v) -> {
        let ty: root.algebra.Ty = root.algebra.EnumLit.new(e.value, v);
        ty
    });
    derived(
        root.algebra.pair(
            root.algebra.Union.new(members),
            root.algebra.Enum.new(e.value),
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
        [l.value.literal, l.value.concrete]
    } else {
        [l.value.concrete, l.value.literal]
    };
    derived(
        root.algebra.pair(
            root.algebra.Union.new(members),
            l.value.concrete,
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
            root.algebra.Union.new([l.value[0], root.algebra.Top {  }]),
            root.algebra.Top {  },
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
            root.algebra.Union.new([l.value[0], root.algebra.Never {  }]),
            l.value[0],
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
            root.algebra.Union.new(l.value),
            root.algebra.Union.new(l.value.reverse()),
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
            root.algebra.Union.new([l.value[0], root.algebra.Union.new(l.value.slice(1, 3))]),
            root.algebra.Union.new(l.value),
            root.algebra.Relation.Equiv,
            union_associative(),
        ),
        l.next,
    )
}

function alias_is_transparent(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 1);
    let n = pick_type_name(l.next);
    let decl = root.algebra.AliasDecl { name: n.value, body: l.value[0] };
    derived(
        root.algebra.pair(
            root.algebra.Alias { decl: decl },
            l.value[0],
            root.algebra.Relation.Equiv,
            alias_transparent(),
        ),
        n.next,
    )
}

/// `type A = leaf | A[]` under a seeded name.
function list_alias(name: string, leaf: root.algebra.Ty) -> root.algebra.AliasDecl throws never {
    root.algebra.AliasDecl.recursive(name, (self) -> {
        root.algebra.Union.new([leaf, root.algebra.Array.new(self)])
    })
}

function equirecursive(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 1);
    let names = root.engine.pick_distinct(l.next, type_names(), 2);
    derived(
        root.algebra.pair(
            root.algebra.Alias { decl: list_alias(names.value[0], l.value[0]) },
            root.algebra.Alias { decl: list_alias(names.value[1], l.value[0]) },
            root.algebra.Relation.Equiv,
            equirecursive_identity(),
        ),
        names.next,
    )
}

function unfolds_once(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 1);
    let n = pick_type_name(l.next);
    let decl = list_alias(n.value, l.value[0]);
    derived(
        root.algebra.pair(
            root.algebra.Alias { decl: decl },
            root.algebra.Union.new(
                [l.value[0], root.algebra.Array.new(root.algebra.Alias { decl: decl })],
            ),
            root.algebra.Relation.Equiv,
            partial_unfolding(),
        ),
        n.next,
    )
}

function unguarded_member_is_vacuous(seed: root.engine.Seed) -> Derived throws never {
    let names = root.engine.pick_distinct(seed, type_names(), 2);
    let with_member = root.algebra.AliasDecl.recursive(names.value[0], (self) -> {
        root.algebra.Union.new([self, root.algebra.Array.new(self)])
    });
    let plain = root.algebra.AliasDecl.recursive(names.value[1], (self) -> {
        root.algebra.Array.new(self)
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
    let names = root.engine.pick_distinct(l.next, binding_names(), 2);
    let required = root.algebra.Named.new(names.value[0], l.value[0], false);
    let more = root.algebra.Fn.new(
        [],
        [required, root.algebra.Named.new(names.value[1], l.value[1], true)],
        l.value[2],
        root.algebra.Never {  },
    );
    let fewer = root.algebra.Fn.new([], [required], l.value[2], root.algebra.Never {  });
    derived(root.algebra.pair(more, fewer, root.algebra.Relation.Sub, fn_optional_superset()), names.next)
}

function positional_count(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 2);
    let one = root.algebra.Fn.new([l.value[0]], [], l.value[1], root.algebra.Never {  });
    let two = root.algebra.Fn.new([l.value[0], l.value[0]], [], l.value[1], root.algebra.Never {  });
    derived(root.algebra.pair(one, two, root.algebra.Relation.Unrelated, fn_positional_count()), l.next)
}

/// Two classes with identical fields are still two types: concrete types are
/// nominal.
function distinct_classes_same_shape(seed: root.engine.Seed) -> Derived throws never {
    let names = root.engine.pick_distinct(seed, type_names(), 2);
    let field = root.engine.pick_distinct(names.next, binding_names(), 1);
    let l = pick_leaves(field.next, 1);
    let shape = [root.algebra.FieldDecl { name: field.value[0], ty: l.value[0] }];
    derived(
        root.algebra.pair(
            root.algebra.Class.new(
                root.algebra.ClassDecl { name: names.value[0], params: [], fields: shape },
                [],
            ),
            root.algebra.Class.new(
                root.algebra.ClassDecl { name: names.value[1], params: [], fields: shape },
                [],
            ),
            root.algebra.Relation.Unrelated,
            concrete_types_atomic(),
        ),
        l.next,
    )
}

// ─── context facts ───────────────────────────────────────────────────────────

/// A seeded strict-subtype pair from one of the pair-introducing facts, so
/// context facts vary in what they wrap. Not a fact of its own.
function strict_pair(seed: root.engine.Seed) -> Derived throws never {
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
    let inner = strict_pair(seed);
    let b = box_decl(inner.next);
    derived(root.algebra.in_class_arg(inner.value, b.value, generics_invariant()), b.next)
}

function array_invariant(seed: root.engine.Seed) -> Derived throws never {
    let inner = strict_pair(seed);
    derived(root.algebra.in_array(inner.value, generics_invariant()), inner.next)
}

function map_value_invariant(seed: root.engine.Seed) -> Derived throws never {
    let inner = strict_pair(seed);
    derived(root.algebra.in_map_value(inner.value, generics_invariant()), inner.next)
}

function param_contravariant(seed: root.engine.Seed) -> Derived throws never {
    let inner = strict_pair(seed);
    let r = pick_leaves(inner.next, 1);
    derived(root.algebra.in_fn_param(inner.value, r.value[0], fn_param_contravariant()), r.next)
}

function return_covariant(seed: root.engine.Seed) -> Derived throws never {
    let inner = strict_pair(seed);
    derived(root.algebra.in_fn_return(inner.value, fn_return_covariant()), inner.next)
}

function error_covariant(seed: root.engine.Seed) -> Derived throws never {
    let inner = strict_pair(seed);
    let r = pick_leaves(inner.next, 1);
    derived(root.algebra.in_fn_error(inner.value, r.value[0], fn_error_covariant()), r.next)
}

function box_of_bool(seed: root.engine.Seed) -> Derived throws never {
    let inner = bool_true_false(seed);
    let b = box_decl(inner.next);
    derived(root.algebra.in_class_arg(inner.value, b.value, generics_invariant()), b.next)
}

function double_contravariance(seed: root.engine.Seed) -> Derived throws never {
    let inner = strict_pair(seed);
    let r = pick_leaves(inner.next, 1);
    let once = root.algebra.in_fn_param(inner.value, r.value[0], fn_param_contravariant());
    derived(root.algebra.in_fn_param(once, r.value[0], fn_param_contravariant()), r.next)
}

function array_of_unfolded_alias(seed: root.engine.Seed) -> Derived throws never {
    let inner = unfolds_once(seed);
    derived(root.algebra.in_array(inner.value, generics_invariant()), inner.next)
}

/// `never | X` against `T | X` for a leaf `X` distinct from `T`: the algebra
/// declines the step when the extra member could absorb the difference.
function bottom_beside_member(seed: root.engine.Seed) -> Derived throws never {
    let inner = never_bottom(seed);
    let extra = pick_leaves(inner.next, 2);
    let other = if (extra.value[0] == root.algebra.right_of(inner.value)) {
        extra.value[1]
    } else {
        extra.value[0]
    };
    derived(root.algebra.in_union_with(inner.value, other, union_member_covariant()), extra.next)
}

// ─── registry ────────────────────────────────────────────────────────────────

function fact(
    name: string,
    generate: (root.engine.Seed) -> Derived throws never,
) -> Fact throws never {
    Fact { name: name, generate: generate }
}

function facts() -> Fact[] throws never {
    [
        fact("never_bottom", never_bottom),
        fact("unknown_top", unknown_top),
        fact("identity", identity),
        fact("union_member", union_member),
        fact("union_subset", union_subset),
        fact("literal_subtype", literal_subtype),
        fact("concrete_atomic", concrete_atomic),
        fact("bool_true_false", bool_true_false),
        fact("enum_variants", enum_variants),
        fact("literal_absorbed_fact", literal_absorbed_fact),
        fact("unknown_absorbs_fact", unknown_absorbs_fact),
        fact("never_omitted", never_omitted),
        fact("union_commutes", union_commutes),
        fact("union_associates", union_associates),
        fact("alias_is_transparent", alias_is_transparent),
        fact("equirecursive", equirecursive),
        fact("unfolds_once", unfolds_once),
        fact("unguarded_member_is_vacuous", unguarded_member_is_vacuous),
        fact("optional_superset", optional_superset),
        fact("positional_count", positional_count),
        fact("distinct_classes_same_shape", distinct_classes_same_shape),
        fact("class_arg_invariant", class_arg_invariant),
        fact("array_invariant", array_invariant),
        fact("map_value_invariant", map_value_invariant),
        fact("param_contravariant", param_contravariant),
        fact("return_covariant", return_covariant),
        fact("error_covariant", error_covariant),
        fact("box_of_bool", box_of_bool),
        fact("double_contravariance", double_contravariance),
        fact("array_of_unfolded_alias", array_of_unfolded_alias),
        fact("bottom_beside_member", bottom_beside_member),
    ]
}

/// Helpers that share a fact's signature but are not facts. The registry
/// check excludes them by name.
function fact_helpers() -> string[] throws never {
    ["strict_pair"]
}
