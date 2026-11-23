function int_count_ones_zero() -> int { count(0).count_ones() }
function int_count_ones_one() -> int { count(1).count_ones() }
function int_count_ones_minus_one() -> int { (0 - count(1)).count_ones() }
function int_count_ones_mixed() -> int { count(1000).count_ones() }
function int_count_ones_negative() -> int { (0 - count(1000)).count_ones() }
function int_count_ones_large() -> int { (wide(61) - 1 + wide(61)).count_ones() }
function int_count_ones_power() -> int { wide(40).count_ones() }
function int_count_ones_of_the_smallest() -> int { (0 - (wide(62) - 1 + wide(62)) - 1).count_ones() }
