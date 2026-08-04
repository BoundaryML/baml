// pending: A4 spawn expressions type as Future (S15 ledger, ~230 nodes)
//
// rustc's async-block shape: the body types under its OWN effect
// channel (the S12 lambda discipline), and the expression is
// `Future<body, union(channel)>`.
class Boom {
    m string
}

function boom() -> int throws user.Boom {
    throw Boom { m: "x" }
}

function f() -> int throws never {
    let a = spawn { 42 };
     // ^ Future<int, never>
    let b = spawn { boom() };
     // ^ Future<int, user.Boom>
    0
}
