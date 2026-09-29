// B-1788: a method receiver demands an operator result before finish.
// Follow the pending operators back to the reduce accumulator's seed so
// method resolution and the call plan are recorded during the body walk.
function rounded_share(values: int[]) -> float {
    let total = values.reduce((sum, n) -> { sum + n }, 0);
    let share = 1 / (total * 1.0);
    let rounded = (share * 100.0).round();
     // ^^^^^^^ float
    rounded / 100.0
}

function negated_sum(values: int[]) -> float {
    let total = values.reduce((sum, n) -> { sum + n }, 0.0);
    let rounded = (-total).round();
     // ^^^^^^^ float
    rounded
}
