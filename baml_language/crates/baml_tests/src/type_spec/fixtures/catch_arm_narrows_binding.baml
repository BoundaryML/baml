// R1 (S15 residue): inside a catch arm, the clause binding narrows to the arm's matched
// type - the match-arm discipline applied to `e` (TIR does this; we
// typed `e` as the whole caught union, so `e.m` was a field-on-union
// error).
class Boom {
    m string
}

class Kapow {
    n int
}

function risky() -> string throws user.Boom | user.Kapow {
    throw Boom { m: "x" }
}

function f() -> string throws never {
    risky() catch (e) {
        Boom => {
            let b = e;
             // ^ user.Boom
            e.m
        },
        Kapow => "kapow",
    }
}
