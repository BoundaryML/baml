// Native backend fixture: checked integer arithmetic and boolean logic.

function arith(a: int, b: int) -> int {
    let sum = a + b;
    let diff = a - b;
    let prod = sum * diff;
    let quot = prod / 3;
    let rem = prod % 7;
    -quot + rem
}

function divide(a: int, b: int) -> int {
    a / b
}

function remainder(a: int, b: int) -> int {
    a % b
}

function add_to_max(a: int) -> int {
    a + 4611686018427387903
}

function negate(a: int) -> int {
    -a
}

function in_range(x: int, lo: int, hi: int) -> bool {
    x >= lo && x <= hi
}

function is_edge(x: int, lo: int, hi: int) -> bool {
    x == lo || x == hi || !(x > lo && x < hi)
}

function compare(a: int, b: int) -> int {
    if (a < b) {
        -1
    } else if (a > b) {
        1
    } else {
        0
    }
}
