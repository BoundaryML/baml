// B-1788: a method receiver demands an operator result before finish.
// Resolve the pending operator from its operand's bounds so method
// resolution and the call plan are recorded during the body walk.
function id<T>(x: T) -> T { x }

function unary_receiver(n: int) -> int {
    let r = (-id(n)).abs();
     // ^ int
    r
}

function binary_receiver(n: int) -> int {
    let r = (id(n) + 1).abs();
     // ^ int
    r
}

// The accumulator is bounded by its own callback output; the cycle must break.
function reduce_receiver(values: int[]) -> float {
    let r = (values.reduce((sum, n) -> { sum + n }, 0) * 1.0).round();
     // ^ float
    r
}

function aliased_receiver(n: int) -> int {
    let r = id(id(-id(n))).abs();
     // ^ int
    r
}
