function int_trailing_zeros_zero() -> int { count(0).trailing_zeros() }
function int_trailing_zeros_one() -> int { count(1).trailing_zeros() }
function int_trailing_zeros_minus_one() -> int { (0 - count(1)).trailing_zeros() }
function int_trailing_zeros_mixed() -> int { count(1000).trailing_zeros() }
function int_trailing_zeros_negative() -> int { (0 - count(1000)).trailing_zeros() }
function int_trailing_zeros_large() -> int { (wide(61) - 1 + wide(61)).trailing_zeros() }
function int_trailing_zeros_power() -> int { wide(40).trailing_zeros() }
function int_trailing_zeros_of_the_smallest() -> int { (0 - (wide(62) - 1 + wide(62)) - 1).trailing_zeros() }
