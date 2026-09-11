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

/// The strings literal types are drawn from. One is multi-byte, so a
/// generated case exercises the byte arithmetic that locates a rejection
/// just as the fixed corpus does.
function literal_words() -> string[] throws never {
    ["sheep", "🐑"]
}

/// A literal type and the concrete type it belongs to.
///
/// The pool is built before the draw that indexes it, so the draw is bounded
/// by the pool's own size: a literal kind cannot be added and left
/// unreachable, as it could when the bound was written out beside the arms.
function pick_literal(seed: root.engine.Seed) -> root.engine.Rolled<LiteralPair> throws never {
    let n = root.engine.below(seed, 100);
    let words = literal_words();
    let pool = [
        LiteralPair { literal: root.algebra.Lit.new(n.value), concrete: root.algebra.Prim.int() },
        LiteralPair {
            literal: root.algebra.Lit.new(words[n.value % words.length()]),
            concrete: root.algebra.Prim.string(),
        },
        LiteralPair {
            literal: root.algebra.Lit.new(n.value % 2 == 0),
            concrete: root.algebra.Prim.bool(),
        },
    ];
    let kind = root.engine.below(n.next, pool.length());
    root.engine.Rolled<LiteralPair> { value: pool[kind.value], next: kind.next }
}

/// A type name that is not one of `reserved`. Every draw says what it must
/// avoid, so a declaration drawn beside others cannot take a name one of them
/// already has — which the case assembly rejects as a generator bug.
function pick_type_name(
    seed: root.engine.Seed,
    reserved: string[],
) -> root.engine.Rolled<string> throws never {
    let pool = type_names().filter((n) -> {
        !reserved.includes(n)
    });
    let picked = root.engine.pick_distinct(seed, pool, 1);
    root.engine.Rolled<string> { value: picked.value[0], next: picked.next }
}

/// `class Name<T> { value: T }` with a seeded name, avoiding `reserved`.
function box_decl(
    seed: root.engine.Seed,
    reserved: string[],
) -> root.engine.Rolled<root.algebra.ClassDecl> throws never {
    let n = pick_type_name(seed, reserved);
    root.engine.Rolled<root.algebra.ClassDecl> {
        value: root.algebra.ClassDecl {
            name: n.value,
            params: ["T"],
            fields: [root.algebra.FieldDecl { name: "value", ty: root.algebra.Var.new("T") }],
            impls: [],
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
    let none: string[] = [];
    let n = pick_type_name(seed, none);
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
    let none: string[] = [];
    let n = pick_type_name(l.next, none);
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
    let none: string[] = [];
    let n = pick_type_name(l.next, none);
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
                root.algebra.ClassDecl {
                    name: names.value[0],
                    params: [],
                    fields: shape,
                    impls: [],
                },
                [],
            ),
            root.algebra.Class.new(
                root.algebra.ClassDecl {
                    name: names.value[1],
                    params: [],
                    fields: shape,
                    impls: [],
                },
                [],
            ),
            root.algebra.Relation.Unrelated,
            concrete_types_atomic(),
        ),
        l.next,
    )
}

// ─── interfaces and generics ─────────────────────────────────────────────────

/// An interface that asks for nothing.
///
/// The bank's interfaces declare no members, which the language admits. Every
/// case here turns on WHICH types implement an interface and never on what it
/// asks of them, so a method would be text a learner has to read past before
/// reaching the question. Members arrive with the cases that are about them.
function iface_decl(name: string) -> root.algebra.IfaceDecl throws never {
    root.algebra.IfaceDecl { name, params: [], assoc: [], bounds: [] }
}

/// A one-field class, implementing `impls`.
function thing_decl(
    name: string,
    field: string,
    ty: root.algebra.Ty,
    impls: root.algebra.Impl[],
) -> root.algebra.ClassDecl throws never {
    root.algebra.ClassDecl {
        name,
        params: [],
        fields: [root.algebra.FieldDecl { name: field, ty }],
        impls,
    }
}

/// A class with an `implements` block, against the interface it implements:
/// the existential stands for exactly the types that implement it, and this
/// is one of them.
function implementor_below_iface(seed: root.engine.Seed) -> Derived throws never {
    let names = root.engine.pick_distinct(seed, type_names(), 2);
    let field = root.engine.pick_distinct(names.next, binding_names(), 1);
    let l = pick_leaves(field.next, 1);
    let iface = iface_decl(names.value[0]);
    let cls = thing_decl(names.value[1], field.value[0], l.value[0], [root.algebra.Impl.of(iface)]);
    derived(
        root.algebra.pair(
            root.algebra.Class.new(cls, []),
            root.algebra.Iface.of(iface),
            root.algebra.Relation.Sub,
            implementor_below_existential(),
        ),
        l.next,
    )
}

/// An interface that requires another, against the one it requires. Every
/// implementor of the first implements the second, so the first existential
/// stands for fewer types.
function requires_widens(seed: root.engine.Seed) -> Derived throws never {
    let names = root.engine.pick_distinct(seed, type_names(), 2);
    let base = iface_decl(names.value[0]);
    let extra = root.algebra.IfaceDecl {
        name: names.value[1],
        params: [],
        assoc: [],
        bounds: [root.algebra.Iface.of(base)],
    };
    derived(
        root.algebra.pair(
            root.algebra.Iface.of(extra),
            root.algebra.Iface.of(base),
            root.algebra.Relation.Sub,
            requires_widens_existential(),
        ),
        names.next,
    )
}

/// A type variable bounded by an interface, against that interface. The
/// variable stands for one implementor, so its values fit the existential;
/// the existential does not fit the variable, which is the half of this
/// people most often have backwards.
function typevar_below_its_bound(seed: root.engine.Seed) -> Derived throws never {
    let names = root.engine.pick_distinct(seed, type_names(), 1);
    let param = root.engine.pick_distinct(names.next, param_names(), 1);
    let iface = iface_decl(names.value[0]);
    derived(
        root.algebra.pair(
            root.algebra.Var.bounded(param.value[0], root.algebra.Iface.of(iface)),
            root.algebra.Iface.of(iface),
            root.algebra.Relation.Sub,
            typevar_below_bound(),
        ),
        param.next,
    )
}

/// A union of two implementors, against the interface both implement. The
/// union fits where the existential is expected without being an implementor
/// itself, and the existential does not fit the union: another package may
/// implement the interface too.
function union_of_implementors(seed: root.engine.Seed) -> Derived throws never {
    let names = root.engine.pick_distinct(seed, type_names(), 3);
    let field = root.engine.pick_distinct(names.next, binding_names(), 1);
    let l = pick_leaves(field.next, 1);
    let iface = iface_decl(names.value[0]);
    let block = [root.algebra.Impl.of(iface)];
    let one = thing_decl(names.value[1], field.value[0], l.value[0], block);
    let two = thing_decl(names.value[2], field.value[0], l.value[0], block);
    derived(
        root.algebra.pair(
            root.algebra.Union.new(
                [root.algebra.Class.new(one, []), root.algebra.Class.new(two, [])],
            ),
            root.algebra.Iface.of(iface),
            root.algebra.Relation.Sub,
            union_below_existential(),
        ),
        l.next,
    )
}

/// A class with no `implements` block, against an interface that asks for
/// nothing: implementation is nominal, so an empty contract is still not
/// satisfied by a type that never declared it.
function non_implementor(seed: root.engine.Seed) -> Derived throws never {
    let names = root.engine.pick_distinct(seed, type_names(), 2);
    let field = root.engine.pick_distinct(names.next, binding_names(), 1);
    let l = pick_leaves(field.next, 1);
    let no_impls: root.algebra.Impl[] = [];
    let iface = iface_decl(names.value[0]);
    let cls = thing_decl(names.value[1], field.value[0], l.value[0], no_impls);
    derived(
        root.algebra.pair(
            root.algebra.Class.new(cls, []),
            root.algebra.Iface.of(iface),
            root.algebra.Relation.Unrelated,
            implements_is_nominal(),
        ),
        l.next,
    )
}

// ─── context facts ───────────────────────────────────────────────────────────

/// The declarations a fact's pair already names, which a wrapper's own
/// declaration must not collide with.
function inner_names(d: root.algebra.Derivation) -> string[] throws never {
    root.algebra.decl_names_of([root.algebra.left_of(d), root.algebra.right_of(d)])
}

/// A seeded strict-subtype pair from one of the pair-introducing facts, so
/// context facts vary in what they wrap. Not a fact of its own.
///
/// The generators are a pool the draw is bounded by, rather than match arms
/// beside a written-out count that has to be kept in step with them.
function strict_pair(seed: root.engine.Seed) -> Derived throws never {
    let pool: ((root.engine.Seed) -> Derived throws never)[] = [
        never_bottom,
        unknown_top,
        union_member,
        union_subset,
        literal_subtype,
        implementor_below_iface,
        requires_widens,
        typevar_below_its_bound,
        union_of_implementors,
    ];
    let choice = root.engine.below(seed, pool.length());
    pool[choice.value](choice.next)
}

function class_arg_invariant(seed: root.engine.Seed) -> Derived throws never {
    let inner = strict_pair(seed);
    let b = box_decl(inner.next, inner_names(inner.value));
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
    let b = box_decl(inner.next, inner_names(inner.value));
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
        fact("implementor_below_iface", implementor_below_iface),
        fact("requires_widens", requires_widens),
        fact("typevar_below_its_bound", typevar_below_its_bound),
        fact("union_of_implementors", union_of_implementors),
        fact("non_implementor", non_implementor),
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
