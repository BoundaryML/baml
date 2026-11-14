// Tuple bracket access (02b "Element access"): the index names one slot
// statically - a literal, a constant fold, or anything of an integer literal
// type - and the access has that slot's type. A union receiver reads the slot
// of every variant (arities may differ) and joins them; `?.[` on an optional
// tuple makes the result optional.
type TisPair = (int, string);

function tis_static(t: (int, string, bool), i: 1) -> bool throws never {
    let a = t[0];
     // ^ int
    let b = t[i];
     // ^ string
    let c = t[i + 1];
     // ^ bool
    let d = t[(2 - 2)];
     // ^ int
    c
}

function tis_union(t: TisPair | (string, bool, int)) -> int | string throws never {
    let a = t[0];
     // ^ int | string
    let b = t[1];
     // ^ string | bool
    a
}

function tis_optional(t: (int?, string)?) -> int? throws never {
    let a = t?.[1];
     // ^ string | null
    let b = t?.[0];
     // ^ int | null
    b
}

function tis_nested(t: ((int, (bool,)), string)) -> bool throws never {
    let a = t[0][1][0];
     // ^ bool
    a
}
