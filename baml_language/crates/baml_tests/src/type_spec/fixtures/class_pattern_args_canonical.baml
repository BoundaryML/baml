// Written class-pattern type args are canonicalized like the scrutinee
//
// The scrutinee is canonical (union members in canonical order, aliases
// expanded), so a destructure whose written args spell the same type
// differently (`Box<B | A>` against `Box<A | B>`) must still name the
// same ctor. Otherwise a fully covering arm reads as a different
// instantiation and the match is wrongly non-exhaustive.
class CaA {
    x int
}

class CaB {
    y string
}

class CaBox<T> {
    v T
}

class CaTwo<T, U> {
    t T
    u U
}

type CaAB = CaA | CaB;

function ca_flat(b: CaBox<CaB | CaA>) -> string throws never {
    let r = match (b) {
     // ^ string
        CaBox<CaB | CaA> { v: _ } => "any",
    };
    r
}

function ca_canonical_order(b: CaBox<CaB | CaA>) -> string throws never {
    let r = match (b) {
     // ^ string
        CaBox<CaA | CaB> { v: _ } => "any",
    };
    r
}

function ca_list(b: CaBox<(CaB | CaA)[]>) -> string throws never {
    let r = match (b) {
     // ^ string
        CaBox<(CaB | CaA)[]> { v: _ } => "any",
    };
    r
}

function ca_map(b: CaBox<map<string, CaB | CaA>>) -> string throws never {
    let r = match (b) {
     // ^ string
        CaBox<map<string, CaB | CaA>> { v: _ } => "any",
    };
    r
}

function ca_nested(b: CaBox<CaBox<CaB | CaA>>) -> string throws never {
    let r = match (b) {
     // ^ string
        CaBox<CaBox<CaB | CaA>> { v: CaBox<CaB | CaA> { v: _ } } => "any",
    };
    r
}

function ca_multi_arg(b: CaTwo<CaB | CaA, bool>) -> string throws never {
    let r = match (b) {
     // ^ string
        CaTwo<CaB | CaA, bool> { t: _, u: _ } => "any",
    };
    r
}

function ca_alias(b: CaBox<CaA | CaB>) -> string throws never {
    let r = match (b) {
     // ^ string
        CaBox<CaAB> { v: _ } => "any",
    };
    r
}

function ca_optional(b: CaBox<CaB | CaA> | null) -> string throws never {
    let r = match (b) {
     // ^ string
        CaBox<CaB | CaA> { v: _ } => "any",
        null => "none",
    };
    r
}

function ca_union(b: CaBox<CaB | CaA> | CaBox<int>) -> string throws never {
    let r = match (b) {
     // ^ string
        CaBox<CaB | CaA> { v: _ } => "any",
        CaBox<int> { v: _ } => "int",
    };
    r
}

function ca_union_field_split(b: CaBox<CaB | CaA> | null) -> string throws never {
    let r = match (b) {
     // ^ string
        CaBox<CaB | CaA> { v: CaA } => "a",
        CaBox<CaB | CaA> { v: CaB } => "b",
        null => "none",
    };
    r
}

function ca_tuple(t: (CaB | CaA, int) | null) -> string throws never {
    let r = match (t) {
     // ^ string
        (CaB | CaA, int) => "pair",
        null => "none",
    };
    r
}

// Canonicalization must not over-cover: the `int` instantiation is still
// missing.
function ca_still_missing(b: CaBox<CaB | CaA> | CaBox<int>) -> string throws never {
    let r = match (b) { CaBox<CaB | CaA> { v: _ } => "any" };
     // ^ !error
//          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ non-exhaustive
    r
}

// Recursive alias args: the canonical form unfolds a root alias once, but
// inside a canonical head the arg stays folded. Ctor args come from the
// canonical head, never from each arg canonicalized on its own.
type CaR = int | CaR[]

type CaJ = CaA | CaB | CaJ[]

interface CaHas<T> {
    v T
}

function ca_rec(b: CaBox<CaR>) -> string throws never {
    let r = match (b) {
     // ^ string
        CaBox<CaR> { v: _ } => "any",
    };
    r
}

function ca_rec_optional(b: CaBox<CaR> | null) -> string throws never {
    let r = match (b) {
     // ^ string
        null => "none",
        CaBox<CaR> { v: _ } => "any",
    };
    r
}

function ca_rec_union(b: CaBox<CaJ>) -> string throws never {
    let r = match (b) {
     // ^ string
        CaBox<CaJ> { v: _ } => "any",
    };
    r
}

function ca_rec_multi_arg(b: CaTwo<CaR, CaJ>) -> string throws never {
    let r = match (b) {
     // ^ string
        CaTwo<CaR, CaJ> { t: _, u: _ } => "any",
    };
    r
}

function ca_rec_nested(b: CaBox<CaBox<CaR>>) -> string throws never {
    let r = match (b) {
     // ^ string
        CaBox<CaBox<CaR>> { v: CaBox<CaR> { v: _ } } => "any",
    };
    r
}

function ca_rec_interface(h: CaHas<CaR> | null) -> string throws never {
    let r = match (h) {
     // ^ string
        null => "none",
        CaHas<CaR> { v: _ } => "any",
    };
    r
}
