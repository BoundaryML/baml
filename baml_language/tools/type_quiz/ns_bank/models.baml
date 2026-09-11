// The naive models: intuitions a learner might arrive with, each a sparse
// list of the rules where it disagrees with the spec. A case is a trap for a
// model when replaying its derivation under the model flips the verdict.
//
// A model is trapped only in the directions it is wrong in. One that accepts
// less than the spec (`spelling`) is trapped by compiling cases alone, one
// that accepts more (`typescript`) by rejected cases alone, and one that
// gets a variance backwards (`covariant`) by both.

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
        let f: root.algebra.Fn => f.positional.length(),
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

// ─── constant beliefs ────────────────────────────────────────────────────────

function unrelated(
    left: root.algebra.Ty,
    right: root.algebra.Ty,
) -> root.algebra.Relation? throws never {
    root.algebra.Relation.Unrelated
}

function subset(
    left: root.algebra.Ty,
    right: root.algebra.Ty,
) -> root.algebra.Relation? throws never {
    root.algebra.Relation.Sub
}

function superset(
    left: root.algebra.Ty,
    right: root.algebra.Ty,
) -> root.algebra.Relation? throws never {
    root.algebra.Relation.Super
}

function equivalent(
    left: root.algebra.Ty,
    right: root.algebra.Ty,
) -> root.algebra.Relation? throws never {
    root.algebra.Relation.Equiv
}

// ─── models ──────────────────────────────────────────────────────────────────

/// Different spelling, different type, with the subtyping rules otherwise
/// intact: a union of literals fits its concrete type but is not it, a
/// union that lists a type beside extra members is strictly bigger than
/// that type, and a reordering, a re-bracketing, an alias, or an unfolding
/// of a recursive alias spells a different type.
function spelling() -> root.algebra.Model throws never {
    root.algebra.Model {
        id: "spelling",
        description: "You treat a type's spelling as its identity: two types written differently are never the same type, though the subtyping rules still apply one at a time. So `true | false` fits where `bool` is expected but is a different, smaller type; `1 | int`, `int | never`, and `int | unknown` each list a type beside more members, so each is strictly bigger than that type; a reordered or re-bracketed union is another union; an alias is a new type; and a recursive alias is only itself, not its unfolding.",
        believes: {
            "subtyping/bool-is-true-or-false": subset,
            "subtyping/enum-is-union-of-variants": subset,
            "subtyping/literal-absorbed": superset,
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
/// are bivariant, a function may take fewer positional parameters than
/// expected, and optional parameters may be present or absent on either
/// side.
function typescript() -> root.algebra.Model throws never {
    root.algebra.Model {
        id: "typescript",
        description: "You reason as TypeScript does: `int` and `float` are both `number`; arrays and generics are covariant, so `int[]` fits where `(int | string)[]` is expected; method parameters are bivariant; a function with fewer positional parameters is accepted where more are expected; and optional parameters are interchangeable with their absence, so adding or dropping one changes nothing.",
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
            "functions/optional-arguments-superset": equivalent,
        },
        transfers: {
            "variance/generics-invariant": root.algebra.covariant,
            "variance/function-parameter-contravariant": bivariant,
        },
    }
}

/// Every constructor preserves subtyping, as arrays do in Java and as many
/// people first assume of functions: a parameter position is covariant too,
/// so contravariance never enters the picture.
function covariant_everywhere() -> root.algebra.Model throws never {
    root.algebra.Model {
        id: "covariant",
        description: "You assume every type constructor preserves subtyping: if `S` is a subtype of `T`, then `S[]` fits where `T[]` is expected, `Box<S>` where `Box<T>` is, and a function taking `S` where a function taking `T` is expected, parameters included. Contravariance never crosses your mind.",
        believes: {},
        transfers: {
            "variance/generics-invariant": root.algebra.covariant,
            "variance/function-parameter-contravariant": root.algebra.covariant,
        },
    }
}

/// An interface bound read as a type annotation: `S extends Base` is taken to
/// mean `S` IS `Base`, so values pass in both directions.
function bounds_are_types() -> root.algebra.Model throws never {
    root.algebra.Model {
        id: "bounds",
        description: "You read `S extends Base` as saying that `S` is `Base`, so the two are interchangeable: a value typed by the bound goes into a slot typed by the variable as readily as the other way round. Only one direction holds — a variable stands for one implementor, and the interface stands for all of them.",
        believes: { "generics/typevar-below-bound": equivalent },
        transfers: {},
    }
}

/// Implementation read as a shape rather than a declaration: whatever fits
/// the contract implements it, declared or not.
function structural_implements() -> root.algebra.Model throws never {
    root.algebra.Model {
        id: "structural",
        description: "You expect a type to implement an interface whenever it satisfies the contract, the way a structurally-typed language works. In BAML implementation is nominal: only an `implements` block makes a type an implementor, so a class that asks for nothing and is asked for nothing still does not implement an empty interface.",
        believes: { "interfaces/implements-is-nominal": subset },
        transfers: {},
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
    [spelling(), typescript(), covariant_everywhere(), bounds_are_types(), structural_implements()]
}
