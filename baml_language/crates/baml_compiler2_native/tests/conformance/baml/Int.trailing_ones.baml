function int_trailing_ones_zero() -> int { count(0).trailing_ones() }
function int_trailing_ones_one() -> int { count(1).trailing_ones() }
function int_trailing_ones_minus_one() -> int { (0 - count(1)).trailing_ones() }
function int_trailing_ones_mixed() -> int { count(1000).trailing_ones() }
function int_trailing_ones_negative() -> int { (0 - count(1000)).trailing_ones() }
function int_trailing_ones_large() -> int { (wide(61) - 1 + wide(61)).trailing_ones() }
function int_trailing_ones_power() -> int { wide(40).trailing_ones() }
function int_trailing_ones_of_the_smallest() -> int { (0 - (wide(62) - 1 + wide(62)) - 1).trailing_ones() }
