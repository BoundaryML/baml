function int_isqrt_square() -> int { count(144).isqrt() }
function int_isqrt_between() -> int { count(150).isqrt() }
function int_isqrt_zero() -> int { count(0).isqrt() }
function int_isqrt_large() -> int { (wide(61) - 1 + wide(61)).isqrt() }
function int_isqrt_negative() -> int { (0 - count(4)).isqrt() }
function int_isqrt_of_the_largest() -> int { (wide(62) - 1 + wide(62)).isqrt() }
