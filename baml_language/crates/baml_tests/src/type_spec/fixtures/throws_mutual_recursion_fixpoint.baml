//
// S12: mutually recursive functions with omitted throws iterate to
// fixpoint from `never` (callable_throws' cycle_initial) - the effect
// each actually contributes is what survives, and the recursive edge
// adds nothing. The name carets pin each function's own inferred
// effect. Thrown literals KEEP their literal types (spec-ahead: TIR
// widens to the base primitive, erasing error codes).
function ping(n: int) -> int {
      // ^^^^ throws "stop"
    if (n == 0) {
        throw "stop";
    }
    pong(n)
}

function pong(n: int) -> int {
      // ^^^^ throws "stop"
    ping(n)
}
