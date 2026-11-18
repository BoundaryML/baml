function int_leading_ones_zero() -> int { count(0).leading_ones() }
function int_leading_ones_one() -> int { count(1).leading_ones() }
function int_leading_ones_minus_one() -> int { (0 - count(1)).leading_ones() }
function int_leading_ones_mixed() -> int { count(1000).leading_ones() }
function int_leading_ones_negative() -> int { (0 - count(1000)).leading_ones() }
function int_leading_ones_large() -> int { (wide(61) - 1 + wide(61)).leading_ones() }
function int_leading_ones_power() -> int { wide(40).leading_ones() }
// native prints: 1
function int_leading_ones_of_the_smallest() -> int { (0 - (wide(62) - 1 + wide(62)) - 1).leading_ones() }
