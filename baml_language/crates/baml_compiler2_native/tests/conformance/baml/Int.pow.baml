function int_pow_small() -> int { count(2).pow(count(10)) }
function int_pow_zero_exponent() -> int { count(2).pow(count(0)) }
function int_pow_zero_to_zero() -> int { count(0).pow(count(0)) }
function int_pow_negative_exponent() -> int { count(2).pow((0 - count(1))) }
function int_pow_negative_base() -> int { (0 - count(2)).pow(count(3)) }
function int_pow_saturates_high() -> int { count(10).pow(count(100)) }
function int_pow_saturates_low() -> int { (0 - count(10)).pow(count(101)) }
function int_pow_saturates_even() -> int { (0 - count(10)).pow(count(100)) }
function int_pow_minus_one_even() -> int { (0 - count(1)).pow(wide(40)) }
function int_pow_minus_one_odd() -> int { (0 - count(1)).pow(wide(40) + 1) }
function int_pow_one_huge_exponent() -> int { count(1).pow(wide(40)) }
function int_pow_two_huge_exponent() -> int { count(2).pow(wide(40)) }
function int_pow_largest_that_fits() -> int { count(2).pow(count(61)) }
function int_pow_first_that_saturates() -> int { count(2).pow(count(62)) }
function int_pow_smallest_that_fits() -> int { (0 - count(2)).pow(count(62)) }
function int_pow_minus_one_negative_exponent() -> int { (0 - count(1)).pow((0 - count(3))) }
