// A recursive alias inside a union column: a member no arm covers is
// reported, and its witness names the missing member.
type R = int | R[];

type Leaf = int;

type LR = Leaf | LR[];

class OBox {
    v: R?,
}

class Box<T> {
    v: T,
}

class Holder {
    v: LR,
}

// Was accepted: the `R[]` member of `R?` had no arm.
function optional_field_missing_list(b: OBox) -> int {
    match (b) {
        OBox { v: null } => 0,
        OBox { v: int } => 1,
    }
}

function generic_optional_field_missing_null(b: Box<R?>) -> int {
    match (b) {
        Box<R?> { v: int } => 1,
        Box<R?> { v: R[] } => 2,
    }
}

function alias_member_field_missing_int(h: Holder) -> int {
    match (h) {
        Holder { v: LR[] } => 2,
    }
}

function union_tuple_slot_missing_string(t: (R | string, bool)) -> int {
    match (t) {
        (int, _) => 1,
        (R[], _) => 2,
    }
}
