function int_abs_positive() -> int { count(7).abs() }
function int_abs_negative() -> int { (0 - count(7)).abs() }
function int_abs_zero() -> int { count(0).abs() }
function int_abs_of_minus_two_to_the_62() -> int { (0 - wide(61) - wide(61)).abs() }
function int_abs_of_the_smallest() -> int { (0 - (wide(62) - 1 + wide(62)) - 1).abs() }
