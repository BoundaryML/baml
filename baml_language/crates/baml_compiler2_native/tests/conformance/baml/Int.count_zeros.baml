function int_count_zeros_zero() -> int { count(0).count_zeros() }
function int_count_zeros_one() -> int { count(1).count_zeros() }
function int_count_zeros_minus_one() -> int { (0 - count(1)).count_zeros() }
function int_count_zeros_mixed() -> int { count(1000).count_zeros() }
function int_count_zeros_negative() -> int { (0 - count(1000)).count_zeros() }
function int_count_zeros_large() -> int { (wide(61) - 1 + wide(61)).count_zeros() }
function int_count_zeros_power() -> int { wide(40).count_zeros() }
// native prints: 63
function int_count_zeros_of_the_smallest() -> int { (0 - (wide(62) - 1 + wide(62)) - 1).count_zeros() }
