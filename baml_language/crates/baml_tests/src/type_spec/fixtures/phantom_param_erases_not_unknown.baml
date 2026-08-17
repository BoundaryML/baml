//
// B-1084 (https://linear.app/boundaryml2/issue/B-1084): a class generic
// param appearing in no field type cannot be determined by
// construction. TIR silently fills `unknown` (the ticket's "silent
// unsoundness"); hir_ty erases the unconstrained var to the Error
// sentinel (ruling 2) - the same treatment the function-call path gets,
// killing the asymmetry. The declaration-site diagnostic (Rust's E0392
// analog) is S17's.
class Phantom<T> {
    label string
}

function main() -> string throws never {
    let p = Phantom { label: "hi" };
     // ^ user.Phantom<!error>
    p.label
}
