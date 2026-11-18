function int_parse_decimal() -> int { int.parse(rep("42", 1)) }
function int_parse_negative() -> int { int.parse(rep("-17", 1)) }
function int_parse_plus() -> int { int.parse(rep("+5", 1)) }
function int_parse_zero() -> int { int.parse(rep("0", 1)) }
function int_parse_empty() -> int { int.parse(rep("x", 0)) }
function int_parse_word() -> int { int.parse(rep("abc", 1)) }
function int_parse_fraction() -> int { int.parse(rep("1.5", 1)) }
function int_parse_padded() -> int { int.parse(rep(" 1", 1)) }
function int_parse_minus() -> int { int.parse(rep("-", 1)) }
function int_parse_largest() -> int { int.parse(rep("4611686018427387903", 1)) }
function int_parse_smallest() -> int { int.parse(rep("-4611686018427387904", 1)) }
function int_parse_one_past_largest() -> int { int.parse(rep("4611686018427387904", 1)) }
function int_parse_one_past_smallest() -> int { int.parse(rep("-4611686018427387905", 1)) }
function int_parse_past_64_bits() -> int { int.parse(rep("9223372036854775808", 1)) }
function int_parse_repeated() -> int { int.parse(rep("12", 4)) }
