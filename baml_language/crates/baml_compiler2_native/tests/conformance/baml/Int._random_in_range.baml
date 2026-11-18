function int_random_in_range_zero_draw() -> int { int._random_in_range(count(0), count(0), count(10)) }
function int_random_in_range_positive_draw() -> int { int._random_in_range(count(12345), count(0), count(10)) }
function int_random_in_range_negative_draw() -> int { int._random_in_range((0 - count(12345)), (0 - count(5)), count(5)) }
function int_random_in_range_single() -> int { int._random_in_range(count(77), count(3), count(4)) }
function int_random_in_range_empty() -> int { int._random_in_range(count(77), count(5), count(5)) }
function int_random_in_range_reversed() -> int { int._random_in_range(count(77), count(9), count(2)) }
function int_random_in_range_rejected() -> int { int._random_in_range((wide(61) - 1 + wide(61)), count(0), count(3)) }
function int_random_in_range_smallest_draw() -> int { int._random_in_range((0 - wide(61) - wide(61)), count(100), count(200)) }
function int_random_in_range_whole_range() -> int { int._random_in_range(count(42), (0 - wide(61) - wide(61)), (wide(61) - 1 + wide(61))) }
function int_random_in_range_large_draw() -> int { int._random_in_range(wide(50) + 7, count(0), count(1000)) }
