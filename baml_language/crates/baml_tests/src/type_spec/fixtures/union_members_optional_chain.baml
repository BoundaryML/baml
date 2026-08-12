// Union-receiver members + optional chains
//
// (TIR types the bare param-union access and the two-step chain
// `unknown` - the spec is ahead on both.)
//
// Member access on a UNION resolves the member on every member type
// and JOINS the results (TS's rule; any member lacking it fails).
// `?.` chains follow TS short-circuit semantics: each `?.` peels null
// from its base and the enclosing OptionalChain boundary unions null
// back into the result - intermediate links see the non-null type.
class UcOk {
    value string
}

class UcWarn {
    value string
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
