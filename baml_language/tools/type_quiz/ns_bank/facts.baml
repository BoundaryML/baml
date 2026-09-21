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

/// Facts a learner cannot tell apart by looking, served as one item: a coin
/// the learner never sees chooses which of them a draw is.
///
/// A fact that answers the same way wherever it flows — an equivalence
/// always compiles, an unrelated pair never does — stands in a shape beside
/// one that wears its shape and answers the other way. The item then answers
/// both ways, and since selection reads the whole lottery rather than the
/// member it drew (`root.engine.Candidate`), nothing that re-weights items —
/// the learner's own progress included — can make a kind of question lean.
/// A strict fact answers both ways on its own, by direction, and stands
/// alone.
class Shape {
    name: string,
    first: Fact,
    rest: Fact[],

    /// Every fact of the shape, in order.
    function members(self) -> Fact[] throws never {
        [self.first].concat(self.rest)
    }
}

/// A fact that is a shape on its own, under its own name.
function alone(fact: Fact) -> Shape throws never {
    Shape { name: fact.name, first: fact, rest: [] }
}

/// Two facts of one shape that answer opposite ways.
function beside(name: string, one: Fact, other: Fact) -> Shape throws never {
    Shape { name, first: one, rest: [other] }
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

// ─── near misses ─────────────────────────────────────────────────────────────
//
// Each fact above that concludes an equivalence reads the same both ways, so
// every case it makes compiles, and a learner who has noticed which shapes
// those are answers them without reasoning. Each fact below wears one of
// those shapes and concludes something else: the shape is no longer the
// answer, and the members have to be read.

/// `A | B` against `B | C`: the members of one are not the members of the
/// other, so reordering is not what happened. Beside `union_commutes`.
function union_member_swapped(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 3);
    derived(
        root.algebra.pair(
            root.algebra.Union.new(l.value.slice(0, 2)),
            root.algebra.Union.new(l.value.slice(1, 3)),
            root.algebra.Relation.Unrelated,
            subset_based(),
        ),
        l.next,
    )
}

/// `A | (B | C)` against `A | B | D`: regrouped, and one member changed.
/// Beside `union_associates`.
function union_regrouped_differs(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 4);
    derived(
        root.algebra.pair(
            root.algebra.Union.new([l.value[0], root.algebra.Union.new(l.value.slice(1, 3))]),
            root.algebra.Union.new([l.value[0], l.value[1], l.value[3]]),
            root.algebra.Relation.Unrelated,
            subset_based(),
        ),
        l.next,
    )
}

/// A literal of one type beside another: `int` against `"sheep" | int`. The
/// literal belongs to no `int`, so nothing absorbs it, and the member is a
/// strict subtype of the union. Beside `literal_absorbed_fact`.
function foreign_literal_beside(seed: root.engine.Seed) -> Derived throws never {
    let lit = pick_literal(seed);
    let l = pick_leaves(lit.next, 2);
    let other = if (l.value[0] == lit.value.concrete) {
        l.value[1]
    } else {
        l.value[0]
    };
    let order = root.engine.below(l.next, 2);
    let members = if (order.value == 0) {
        [lit.value.literal, other]
    } else {
        [other, lit.value.literal]
    };
    derived(
        root.algebra.pair(
            other,
            root.algebra.Union.new(members),
            root.algebra.Relation.Sub,
            union_member_rule(),
        ),
        order.next,
    )
}

/// `int` against `int | unknown`: the union is `unknown`, which holds more
/// than `int` does. Beside `unknown_absorbs_fact`.
function unknown_beside_member(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 1);
    derived(
        root.algebra.pair(
            l.value[0],
            root.algebra.Union.new([l.value[0], root.algebra.Top {  }]),
            root.algebra.Relation.Sub,
            union_member_rule(),
        ),
        l.next,
    )
}

/// `true | int` against `bool`: a `bool` literal beside a type that holds no
/// `bool` does not make `bool`. Beside `bool_true_false`.
///
/// The other member is a whole type rather than a literal of one: an `int`
/// or `string` literal here would sit in a case that always rejects, with
/// nothing that answers the other way to set against it, and a learner would
/// come to read a literal in a union as a rejection.
function bool_beside_other_type(seed: root.engine.Seed) -> Derived throws never {
    let which = root.engine.below(seed, 2);
    let others = leaves().filter((leaf) -> {
        leaf != root.algebra.Prim.bool()
    });
    let other = root.engine.below(which.next, others.length());
    let order = root.engine.below(other.next, 2);
    let members = if (order.value == 0) {
        [root.algebra.Lit.new(which.value == 0), others[other.value]]
    } else {
        [others[other.value], root.algebra.Lit.new(which.value == 0)]
    };
    derived(
        root.algebra.pair(
            root.algebra.Union.new(members),
            root.algebra.Prim.bool(),
            root.algebra.Relation.Unrelated,
            subset_based(),
        ),
        order.next,
    )
}

/// One variant against its enum: a literal below its concrete type. Beside
/// `enum_variants`.
function enum_variant_below_enum(seed: root.engine.Seed) -> Derived throws never {
    let e = pick_enum(seed);
    let pick = root.engine.below(e.next, e.value.variants.length());
    derived(
        root.algebra.pair(
            root.algebra.EnumLit.new(e.value, e.value.variants[pick.value]),
            root.algebra.Enum.new(e.value),
            root.algebra.Relation.Sub,
            literal_subtype_of_concrete(),
        ),
        pick.next,
    )
}

/// A variant of one enum beside a variant of another, against the first
/// enum: `Side.Left | Shade.Red` is not `Side`, which holds its other
/// variants and no `Shade`. Beside `enum_variants`.
function variants_of_two_enums(seed: root.engine.Seed) -> Derived throws never {
    let first = pick_enum(seed);
    let name = pick_type_name(first.next, [first.value.name]);
    let variants = root.engine.pick_distinct(name.next, variant_names(), 2);
    let second = root.algebra.EnumDecl { name: name.value, variants: variants.value };
    let order = root.engine.below(variants.next, 2);
    let own: root.algebra.Ty = root.algebra.EnumLit.new(first.value, first.value.variants[0]);
    let other: root.algebra.Ty = root.algebra.EnumLit.new(second, second.variants[0]);
    let members = if (order.value == 0) {
        [own, other]
    } else {
        [other, own]
    };
    derived(
        root.algebra.pair(
            root.algebra.Union.new(members),
            root.algebra.Enum.new(first.value),
            root.algebra.Relation.Unrelated,
            subset_based(),
        ),
        order.next,
    )
}

/// A literal against a concrete type it does not belong to: `1` is an `int`
/// and no `string`. Beside `literal_subtype`.
function literal_of_another_type(seed: root.engine.Seed) -> Derived throws never {
    let lit = pick_literal(seed);
    let l = pick_leaves(lit.next, 2);
    let other = if (l.value[0] == lit.value.concrete) {
        l.value[1]
    } else {
        l.value[0]
    };
    derived(
        root.algebra.pair(
            lit.value.literal,
            other,
            root.algebra.Relation.Unrelated,
            literal_subtype_of_concrete(),
        ),
        l.next,
    )
}

/// `A | never` against `B`: `never` adds nothing to `A`, so this is `A`
/// against a different type. Beside `never_omitted`.
function never_beside_other(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 2);
    let order = root.engine.below(l.next, 2);
    let members: root.algebra.Ty[] = if (order.value == 0) {
        [l.value[0], root.algebra.Never {  }]
    } else {
        [root.algebra.Never {  }, l.value[0]]
    };
    derived(
        root.algebra.pair(
            root.algebra.Union.new(members),
            l.value[1],
            root.algebra.Relation.Unrelated,
            subset_based(),
        ),
        order.next,
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

/// `type Id = int` against `string`: the alias is `int`, and that is not
/// `string`. Beside `alias_is_transparent`.
function alias_of_another_leaf(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 2);
    let none: string[] = [];
    let n = pick_type_name(l.next, none);
    let decl = root.algebra.AliasDecl { name: n.value, body: l.value[0] };
    derived(
        root.algebra.pair(
            root.algebra.Alias { decl: decl },
            l.value[1],
            root.algebra.Relation.Unrelated,
            subset_based(),
        ),
        n.next,
    )
}

/// `type Id = int` against `int | string`: the alias is the member. Beside
/// `alias_is_transparent`, and strict, so it answers both ways.
function alias_below_union(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 2);
    let none: string[] = [];
    let n = pick_type_name(l.next, none);
    let decl = root.algebra.AliasDecl { name: n.value, body: l.value[0] };
    derived(
        root.algebra.pair(
            root.algebra.Alias { decl: decl },
            root.algebra.Union.new(l.value),
            root.algebra.Relation.Sub,
            union_member_rule(),
        ),
        n.next,
    )
}

/// `type A = int | A[]` against `type B = string | B[]`: the same recursion
/// over different leaves. Beside `equirecursive`.
function recursive_leaves_differ(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 2);
    let names = root.engine.pick_distinct(l.next, type_names(), 2);
    derived(
        root.algebra.pair(
            root.algebra.Alias { decl: list_alias(names.value[0], l.value[0]) },
            root.algebra.Alias { decl: list_alias(names.value[1], l.value[1]) },
            root.algebra.Relation.Unrelated,
            subset_based(),
        ),
        names.next,
    )
}

/// `A` against `string | A[]` for `type A = int | A[]`: an unfolding keeps
/// the leaf, and this one did not. Beside `unfolds_once`.
function unfolded_with_wrong_leaf(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 2);
    let none: string[] = [];
    let n = pick_type_name(l.next, none);
    let decl = list_alias(n.value, l.value[0]);
    derived(
        root.algebra.pair(
            root.algebra.Alias { decl: decl },
            root.algebra.Union.new(
                [l.value[1], root.algebra.Array.new(root.algebra.Alias { decl: decl })],
            ),
            root.algebra.Relation.Unrelated,
            subset_based(),
        ),
        n.next,
    )
}

/// `type A = A | A[]` against `type L = int | L[]`: the unguarded member is
/// vacuous and a leaf is not, so the second holds every `int` and the first
/// holds none. Beside `unguarded_member_is_vacuous`.
function vacuous_beside_guarded(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 1);
    let names = root.engine.pick_distinct(l.next, type_names(), 2);
    let with_member = root.algebra.AliasDecl.recursive(names.value[0], (self) -> {
        root.algebra.Union.new([self, root.algebra.Array.new(self)])
    });
    derived(
        root.algebra.pair(
            root.algebra.Alias { decl: with_member },
            root.algebra.Alias { decl: list_alias(names.value[1], l.value[0]) },
            root.algebra.Relation.Unrelated,
            subset_based(),
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

/// Two function types with the same positional parameters: the same count,
/// and each a subtype of the other, so each is a subtype of the other.
/// Beside `positional_count`, where the count differs and nothing else does.
function same_positional_count(seed: root.engine.Seed) -> Derived throws never {
    let l = pick_leaves(seed, 2);
    let count = root.engine.below(l.next, 2);
    let params = if (count.value == 0) {
        [l.value[0]]
    } else {
        [l.value[0], l.value[0]]
    };
    let one = root.algebra.Fn.new(params, [], l.value[1], root.algebra.Never {  });
    let other = root.algebra.Fn.new(params, [], l.value[1], root.algebra.Never {  });
    derived(root.algebra.pair(one, other, root.algebra.Relation.Equiv, fn_positional_count()), count.next)
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

/// A bounded type variable against itself: one type. Not a fact of its own —
/// on its own it could only compile — but a member of `equivalent_pair`, so
/// that an invariant container carries a variable to either answer: without
/// it `Box<S>` against `Box<Base>` would be the only variable a container
/// held, and a bounded variable in a container would mean "rejected".
function typevar_is_itself(seed: root.engine.Seed) -> Derived throws never {
    let names = root.engine.pick_distinct(seed, type_names(), 1);
    let param = root.engine.pick_distinct(names.next, param_names(), 1);
    let variable = root.algebra.Var.bounded(param.value[0], root.algebra.Iface.of(iface_decl(names.value[0])));
    derived(
        root.algebra.pair(variable, variable, root.algebra.Relation.Equiv, identity_rule()),
        param.next,
    )
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

/// One class against itself. Beside `distinct_classes_same_shape`, where the
/// fields are as alike and the names are not: what makes two class types one
/// type is the declaration, so the names are what there is to read.
function same_class(seed: root.engine.Seed) -> Derived throws never {
    let names = root.engine.pick_distinct(seed, type_names(), 1);
    let field = root.engine.pick_distinct(names.next, binding_names(), 1);
    let l = pick_leaves(field.next, 1);
    let no_impls: root.algebra.Impl[] = [];
    let cls = thing_decl(names.value[0], field.value[0], l.value[0], no_impls);
    derived(
        root.algebra.pair(
            root.algebra.Class.new(cls, []),
            root.algebra.Class.new(cls, []),
            root.algebra.Relation.Equiv,
            identity_rule(),
        ),
        l.next,
    )
}

/// An implementor beside its own interface, against the interface: the
/// existential already stands for every implementor, so naming one of them
/// beside it adds nothing, as `1 | int` adds nothing to `int`. Beside
/// `non_implementor`.
///
/// Kept out of `equivalent_pair`: that pool reaches the coherence site
/// through a class argument, and whether the overlap check reads
/// `Box<Thing | Base>` as `Box<Base>` is a question of its own.
function implementor_beside_iface(seed: root.engine.Seed) -> Derived throws never {
    let names = root.engine.pick_distinct(seed, type_names(), 2);
    let field = root.engine.pick_distinct(names.next, binding_names(), 1);
    let l = pick_leaves(field.next, 1);
    let iface = iface_decl(names.value[0]);
    let cls = thing_decl(names.value[1], field.value[0], l.value[0], [root.algebra.Impl.of(iface)]);
    let order = root.engine.below(l.next, 2);
    let thing: root.algebra.Ty = root.algebra.Class.new(cls, []);
    let base: root.algebra.Ty = root.algebra.Iface.of(iface);
    let members = if (order.value == 0) {
        [thing, base]
    } else {
        [base, thing]
    };
    derived(
        root.algebra.pair(
            root.algebra.Union.new(members),
            base,
            root.algebra.Relation.Equiv,
            implementor_below_existential(),
        ),
        order.next,
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

/// A strict pair as the argument of a generic class. Invariance collapses a
/// strict relation, so the two class types are unrelated whichever way the
/// pair ran. Beside `class_arg_of_equivalence`, where invariance carries an
/// equivalence through: whether `Box<S>` is `Box<T>` turns on whether `S`
/// is `T`, and the two facts are the two answers.
function class_arg_invariant(seed: root.engine.Seed) -> Derived throws never {
    let inner = strict_pair(seed);
    let b = box_decl(inner.next, inner_names(inner.value));
    derived(root.algebra.in_class_arg(inner.value, b.value, generics_invariant()), b.next)
}

/// Two types the spec calls one type, as the argument of a generic class.
///
/// Invariance carries an equivalence through, so the two class types are one
/// type as well. That matters twice over: a flow site accepts it in both
/// directions, and a class type may be named by an `implements` block where
/// a union or a literal may not, so this is what carries the equivalence
/// facts to the site where being one type is the objection rather than the
/// permission.
function class_arg_of_equivalence(seed: root.engine.Seed) -> Derived throws never {
    let inner = equivalent_pair(seed);
    let b = box_decl(inner.next, inner_names(inner.value));
    derived(root.algebra.in_class_arg(inner.value, b.value, generics_invariant()), b.next)
}

/// A strict pair as an array's element: unrelated arrays. Beside
/// `array_of_equivalence`.
function array_invariant(seed: root.engine.Seed) -> Derived throws never {
    let inner = strict_pair(seed);
    derived(root.algebra.in_array(inner.value, generics_invariant()), inner.next)
}

/// Two types the spec calls one type as an array's element: one array type.
function array_of_equivalence(seed: root.engine.Seed) -> Derived throws never {
    let inner = equivalent_pair(seed);
    derived(root.algebra.in_array(inner.value, generics_invariant()), inner.next)
}

/// `T[]` against `unknown[]`: every `T` is an `unknown`, and an array of one
/// is still not an array of the other. The case of `array_invariant` people
/// are surest of, and the one fact in which `unknown` is rejected both ways —
/// everything flows into `unknown` itself, so without it a learner could
/// read `unknown` as "compiles".
function array_of_unknown(seed: root.engine.Seed) -> Derived throws never {
    let inner = unknown_top(seed);
    derived(root.algebra.in_array(inner.value, generics_invariant()), inner.next)
}

/// A strict pair as a map's value: unrelated maps. Beside
/// `map_value_of_equivalence`.
function map_value_invariant(seed: root.engine.Seed) -> Derived throws never {
    let inner = strict_pair(seed);
    derived(root.algebra.in_map_value(inner.value, generics_invariant()), inner.next)
}

/// Two types the spec calls one type as a map's value: one map type.
function map_value_of_equivalence(seed: root.engine.Seed) -> Derived throws never {
    let inner = equivalent_pair(seed);
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

/// A seeded pair the spec calls one type, from one of the facts that conclude
/// an equivalence. Not a fact of its own.
///
/// The generators are a pool the draw is bounded by, as `strict_pair`'s are.
function equivalent_pair(seed: root.engine.Seed) -> Derived throws never {
    let pool: ((root.engine.Seed) -> Derived throws never)[] = [
        identity,
        bool_true_false,
        enum_variants,
        literal_absorbed_fact,
        unknown_absorbs_fact,
        never_omitted,
        union_commutes,
        union_associates,
        alias_is_transparent,
        equirecursive,
        unfolds_once,
        unguarded_member_is_vacuous,
        typevar_is_itself,
    ];
    let choice = root.engine.below(seed, pool.length());
    pool[choice.value](choice.next)
}

function double_contravariance(seed: root.engine.Seed) -> Derived throws never {
    let inner = strict_pair(seed);
    let r = pick_leaves(inner.next, 1);
    let once = root.algebra.in_fn_param(inner.value, r.value[0], fn_param_contravariant());
    derived(root.algebra.in_fn_param(once, r.value[0], fn_param_contravariant()), r.next)
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

// ─── depth ───────────────────────────────────────────────────────────────────

/// The context wraps that CARRY a relation rather than collapsing it.
///
/// Depth is only a question through transfers that preserve what went in: a
/// covariant step keeps the relation and a contravariant one turns it round,
/// so a learner has to track it through every layer and count the turns.
/// Invariance screens off everything below it (`root.algebra.screens`), so
/// stacking it adds text and no question — a reader who knows only that the
/// two types are written differently answers it. That is why it is not here.
function carrying_wraps(
) -> ((root.algebra.Derivation, root.engine.Seed) -> Derived throws never)[] throws never {
    [wrap_in_param, wrap_in_return, wrap_in_error]
}

function wrap_in_param(d: root.algebra.Derivation, seed: root.engine.Seed) -> Derived throws never {
    let r = pick_leaves(seed, 1);
    derived(root.algebra.in_fn_param(d, r.value[0], fn_param_contravariant()), r.next)
}

function wrap_in_return(
    d: root.algebra.Derivation,
    seed: root.engine.Seed,
) -> Derived throws never {
    derived(root.algebra.in_fn_return(d, fn_return_covariant()), seed)
}

function wrap_in_error(d: root.algebra.Derivation, seed: root.engine.Seed) -> Derived throws never {
    let r = pick_leaves(seed, 1);
    derived(root.algebra.in_fn_error(d, r.value[0], fn_error_covariant()), r.next)
}

/// A strict pair carried through `depth` function positions.
///
/// Every layer keeps the relation, so no layer can be skipped: the verdict
/// turns on the parity of the contravariant ones, and a learner who loses
/// count anywhere gets it wrong. This is the deep end of the bank, and the
/// one place a case cannot be answered by a single reflex. Not a fact of its
/// own: the facts below fix the depth, so that each depth is an item the
/// sampler can tell apart and selection can reach for by name.
function carried(depth: int, seed: root.engine.Seed) -> Derived throws never {
    let inner = strict_pair(seed);
    let pool = carrying_wraps();
    baml.iter.Range.new(0, depth).reduce(
        (so_far, i) -> {
            let pick = root.engine.below(so_far.next, pool.length());
            pool[pick.value](so_far.value, pick.next)
        },
        inner,
    )
}

function carried_2(seed: root.engine.Seed) -> Derived throws never {
    carried(2, seed)
}

function carried_3(seed: root.engine.Seed) -> Derived throws never {
    carried(3, seed)
}

function carried_4(seed: root.engine.Seed) -> Derived throws never {
    carried(4, seed)
}

// ─── registry ────────────────────────────────────────────────────────────────

function fact(
    name: string,
    generate: (root.engine.Seed) -> Derived throws never,
) -> Fact throws never {
    Fact { name: name, generate: generate }
}

/// Every shape the bank serves, one item each.
///
/// A strict fact stands alone: its direction is its coin. Every other fact
/// stands beside one of its own shape that answers the other way, so no
/// item answers one way, which the bank suite holds to.
function shapes() -> Shape[] throws never {
    [
        // Strict: the direction a pair flows decides the answer.
        alone(fact("never_bottom", never_bottom)),
        alone(fact("unknown_top", unknown_top)),
        alone(fact("union_member", union_member)),
        alone(fact("union_subset", union_subset)),
        alone(fact("literal_subtype", literal_subtype)),
        alone(fact("foreign_literal_beside", foreign_literal_beside)),
        alone(fact("unknown_beside_member", unknown_beside_member)),
        alone(fact("enum_variant_below_enum", enum_variant_below_enum)),
        alone(fact("alias_below_union", alias_below_union)),
        alone(fact("optional_superset", optional_superset)),
        alone(fact("implementor_below_iface", implementor_below_iface)),
        alone(fact("requires_widens", requires_widens)),
        alone(fact("typevar_below_its_bound", typevar_below_its_bound)),
        alone(fact("union_of_implementors", union_of_implementors)),
        alone(fact("param_contravariant", param_contravariant)),
        alone(fact("return_covariant", return_covariant)),
        alone(fact("error_covariant", error_covariant)),
        alone(fact("double_contravariance", double_contravariance)),
        alone(fact("carried_2", carried_2)),
        alone(fact("carried_3", carried_3)),
        alone(fact("carried_4", carried_4)),
        alone(fact("bottom_beside_member", bottom_beside_member)),
        // One type, beside a near miss of its shape that is not.
        beside(
            "union_reordered",
            fact("union_commutes", union_commutes),
            fact("union_member_swapped", union_member_swapped),
        ),
        beside(
            "union_regrouped",
            fact("union_associates", union_associates),
            fact("union_regrouped_differs", union_regrouped_differs),
        ),
        beside(
            "bool_as_literals",
            fact("bool_true_false", bool_true_false),
            fact("bool_beside_other_type", bool_beside_other_type),
        ),
        beside(
            "enum_as_variants",
            fact("enum_variants", enum_variants),
            fact("variants_of_two_enums", variants_of_two_enums),
        ),
        beside(
            "literal_beside_concrete",
            fact("literal_absorbed_fact", literal_absorbed_fact),
            fact("literal_of_another_type", literal_of_another_type),
        ),
        beside(
            "never_in_union",
            fact("never_omitted", never_omitted),
            fact("never_beside_other", never_beside_other),
        ),
        beside(
            "unknown_holds",
            fact("unknown_absorbs_fact", unknown_absorbs_fact),
            fact("array_of_unknown", array_of_unknown),
        ),
        beside("two_leaves", fact("identity", identity), fact("concrete_atomic", concrete_atomic)),
        beside(
            "alias_of_leaf",
            fact("alias_is_transparent", alias_is_transparent),
            fact("alias_of_another_leaf", alias_of_another_leaf),
        ),
        beside(
            "two_recursions",
            fact("equirecursive", equirecursive),
            fact("recursive_leaves_differ", recursive_leaves_differ),
        ),
        beside(
            "unfolding",
            fact("unfolds_once", unfolds_once),
            fact("unfolded_with_wrong_leaf", unfolded_with_wrong_leaf),
        ),
        beside(
            "unguarded_member",
            fact("unguarded_member_is_vacuous", unguarded_member_is_vacuous),
            fact("vacuous_beside_guarded", vacuous_beside_guarded),
        ),
        beside(
            "positional_arity",
            fact("same_positional_count", same_positional_count),
            fact("positional_count", positional_count),
        ),
        beside(
            "two_classes",
            fact("same_class", same_class),
            fact("distinct_classes_same_shape", distinct_classes_same_shape),
        ),
        beside(
            "class_and_interface",
            fact("implementor_beside_iface", implementor_beside_iface),
            fact("non_implementor", non_implementor),
        ),
        // Invariance carries an equivalence and collapses a strict relation.
        beside(
            "class_arg",
            fact("class_arg_of_equivalence", class_arg_of_equivalence),
            fact("class_arg_invariant", class_arg_invariant),
        ),
        beside(
            "array_element",
            fact("array_of_equivalence", array_of_equivalence),
            fact("array_invariant", array_invariant),
        ),
        beside(
            "map_value",
            fact("map_value_of_equivalence", map_value_of_equivalence),
            fact("map_value_invariant", map_value_invariant),
        ),
    ]
}

/// Every fact, as the shapes hold them.
function facts() -> Fact[] throws never {
    shapes().flat_map((shape) -> {
        shape.members()
    })
}

/// Helpers that share a fact's signature but are not facts. The registry
/// check excludes them by name.
function fact_helpers() -> string[] throws never {
    [
        "strict_pair",
        "equivalent_pair",
        "typevar_is_itself",
        "wrap_in_param",
        "wrap_in_return",
        "wrap_in_error",
    ]
}
