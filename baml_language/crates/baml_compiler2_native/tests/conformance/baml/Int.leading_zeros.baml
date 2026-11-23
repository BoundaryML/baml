function int_leading_zeros_zero() -> int { count(0).leading_zeros() }
function int_leading_zeros_one() -> int { count(1).leading_zeros() }
function int_leading_zeros_minus_one() -> int { (0 - count(1)).leading_zeros() }
function int_leading_zeros_mixed() -> int { count(1000).leading_zeros() }
function int_leading_zeros_negative() -> int { (0 - count(1000)).leading_zeros() }
function int_leading_zeros_large() -> int { (wide(61) - 1 + wide(61)).leading_zeros() }
function int_leading_zeros_power() -> int { wide(40).leading_zeros() }
function int_leading_zeros_of_the_smallest() -> int { (0 - (wide(62) - 1 + wide(62)) - 1).leading_zeros() }
