// Union-receiver interface members + optional chains
//
// A union exposes a member only through an interface shared by every arm.
// `?.` chains follow TS short-circuit semantics: each `?.` peels null
// from its base and the enclosing OptionalChain boundary unions null
// back into the result - intermediate links see the non-null type.
interface UcValue {
    value string
}

class UcOk {
    value string
    implements UcValue {}
}

class UcWarn {
    value string
    implements UcValue {}
}

function uc_field(r: UcOk | UcWarn) -> string throws never {
    let v = r.value;
     // ^ string
    v
}

function uc_iflet(r: UcOk | UcWarn | int) -> string throws never {
    if let s: UcOk | UcWarn = r {
        s.value
    } else {
        "none"
    }
}

function oc_member(x: UcOk | null) -> string | null throws never {
    let n = x?.value;
     // ^ string | null
    n
}

function oc_call(xs: int[] | null) -> int throws never {
    let n = xs?.length() ?? -1;
     // ^ int
    n
}

function oc_two_step(x: UcOk | null) -> int throws never {
    let n = x?.value.length() ?? 0;
     // ^ int
    n
}
