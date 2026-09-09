// The naive models: intuitions a learner might arrive with, each a sparse
// list of the rules where it disagrees with the spec. A case is a trap for a
// model when replaying its derivation under the model flips the verdict.

function is_numeric(ty: root.algebra.Ty) -> bool throws never {
    match (ty) {
        let p: root.algebra.Prim => match (p.kind) {
            root.algebra.PrimKind.Int => true,
            root.algebra.PrimKind.Float => true,
            root.algebra.PrimKind.Bool => false,
            root.algebra.PrimKind.String => false,
            root.algebra.PrimKind.Bigint => false,
            root.algebra.PrimKind.Null => false,
            root.algebra.PrimKind.Uint8Array => false,
        },
        root.algebra.Never => false,
        root.algebra.Top => false,
        root.algebra.Lit => false,
        root.algebra.EnumLit => false,
        root.algebra.Union => false,
        root.algebra.Array => false,
        root.algebra.Map => false,
        root.algebra.Fn => false,
        root.algebra.Class => false,
        root.algebra.Enum => false,
        root.algebra.Iface => false,
        root.algebra.Alias => false,
        root.algebra.Var => false,
    }
}

function positional_arity(ty: root.algebra.Ty) -> int? throws never {
    match (ty) {
        let f: root.algebra.Fn => {
            f.params
                .filter((p) -> {
                    p is root.algebra.Positional
                })
                .length()
        },
        root.algebra.Prim => null,
        root.algebra.Never => null,
        root.algebra.Top => null,
        root.algebra.Lit => null,
        root.algebra.EnumLit => null,
        root.algebra.Union => null,
        root.algebra.Array => null,
        root.algebra.Map => null,
        root.algebra.Class => null,
        root.algebra.Enum => null,
        root.algebra.Iface => null,
        root.algebra.Alias => null,
        root.algebra.Var => null,
    }
}

function unrelated(
    left: root.algebra.Ty,
    right: root.algebra.Ty,
) -> root.algebra.Relation? throws never {
    root.algebra.Relation.Unrelated
}

function superset(
    left: root.algebra.Ty,
    right: root.algebra.Ty,
) -> root.algebra.Relation? throws never {
    root.algebra.Relation.Super
}

/// Different spelling, different type. Equivalence is syntactic identity,
/// so every algebraic identity the spec states is a surprise.
function spelling() -> root.algebra.Model throws never {
    root.algebra.Model {
        id: "spelling",
        description: "You treat a type's spelling as its identity: two types written differently are different types, so `true | false` is not `bool`, an alias is a new type, a reordered union is another union, and a recursive alias is only itself.",
        believes: {
            "subtyping/bool-is-true-or-false": unrelated,
            "subtyping/enum-is-union-of-variants": unrelated,
            "subtyping/literal-absorbed": unrelated,
            "subtyping/unknown-absorbs": superset,
            "never/omitted-from-unions": superset,
            "subtyping/union-commutative": unrelated,
            "subtyping/union-associative": unrelated,
            "aliases/transparent": unrelated,
            "aliases/equirecursive-identity": unrelated,
            "aliases/partial-unfolding": unrelated,
            "aliases/unguarded-member-vacuous": unrelated,
        },
        transfers: {},
    }
}

/// Numbers are one type, containers and generics are covariant, parameters
/// are bivariant, and a function may take fewer parameters than expected.
function typescript() -> root.algebra.Model throws never {
    root.algebra.Model {
        id: "typescript",
        description: "You reason as TypeScript does: `int` and `float` are both `number`, arrays and generics are covariant so `int[]` fits where `(int | string)[]` is expected, method parameters are bivariant, and a function with fewer parameters is accepted where more are expected.",
        believes: {
            "taxonomy/concrete-types-atomic": (left, right) -> {
                if (is_numeric(left) && is_numeric(right)) {
                    root.algebra.Relation.Equiv
                } else {
                    null
                }
            },
            "functions/same-positional-count": (left, right) -> {
                let l = positional_arity(left)
                    ?? return null;
                let r = positional_arity(right)
                    ?? return null;
                if (l < r) {
                    root.algebra.Relation.Sub
                } else if (l > r) {
                    root.algebra.Relation.Super
                } else {
                    null
                }
            },
        },
        transfers: {
            "variance/generics-invariant": root.algebra.covariant,
            "variance/function-parameter-contravariant": bivariant,
        },
    }
}

/// Either direction of a relation is accepted, as TypeScript's method
/// parameters are.
function bivariant(k: root.algebra.Knowledge) -> root.algebra.Knowledge throws never {
    match (k) {
        root.algebra.Relation.Sub => root.algebra.Relation.Equiv,
        root.algebra.Relation.Super => root.algebra.Relation.Equiv,
        root.algebra.Relation.Equiv => root.algebra.Relation.Equiv,
        root.algebra.Relation.Unrelated => root.algebra.Relation.Unrelated,
        root.algebra.Undecided => root.algebra.Undecided {  },
    }
}

function models() -> root.algebra.Model[] throws never {
    [spelling(), typescript()]
}
