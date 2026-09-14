// let-else is a refutable pattern TEST
//
// `let o: Ok = r else { .. }` is Rust's let-else: the ascription is a
// refutable type test against the synthesized initializer (narrowing
// to the matched type, the else arm covering the residual) - never a
// CHECK of the initializer against the written type.
class LeOk {
    value string
}

class LeErr {
    message string
}

function le_pick(r: LeOk | LeErr) -> string throws never {
    let o: LeOk = r else { return "err"; };
     // ^ user.LeOk
    o.value
}

function le_loop(items: (LeOk | LeErr)[]) -> string throws never {
    let acc = "";
    for (let item in items) {
        let o: LeOk = item else { continue; };
        acc = acc + o.value;
    }
    acc
}
