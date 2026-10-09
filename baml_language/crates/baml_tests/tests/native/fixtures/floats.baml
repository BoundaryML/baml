// Native backend fixture: float arithmetic, the total order and conversions.

function fsum(a: float, b: float) -> float { a + b }

function fcmp(a: float, b: float) -> int {
    if (a < b) { -1 } else if (a > b) { 1 } else if (a == b) { 0 } else { 99 }
}

function fmix(x: float) -> string { (x * 2.5 - 1.0).to_string() }

function ffloor(x: float) -> float { x.floor() }

function fitrunc(x: float) -> int { x.itrunc() }

function nan_order(x: float) -> bool {
    let n = 0.0 / 0.0;
    x < n
}

function fneg(x: float) -> float { -x }

function fdiv(a: float, b: float) -> float { a / b }
