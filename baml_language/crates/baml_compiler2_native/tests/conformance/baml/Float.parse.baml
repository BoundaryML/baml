function float_parse_decimal() -> float { float.parse(rep("1.5", 1)) }
function float_parse_whole() -> float { float.parse(rep("42", 1)) }
function float_parse_negative() -> float { float.parse(rep("-0.25", 1)) }
function float_parse_exponent() -> float { float.parse(rep("1e3", 1)) }
function float_parse_infinite() -> float { float.parse(rep("inf", 1)) }
function float_parse_nan() -> float { float.parse(rep("NaN", 1)) }
function float_parse_overflow() -> float { float.parse(rep("1e999", 1)) }
function float_parse_empty() -> float { float.parse(rep("x", 0)) }
function float_parse_word() -> float { float.parse(rep("abc", 1)) }
function float_parse_padded() -> float { float.parse(rep(" 1.5", 1)) }
function float_parse_quoted() -> float { float.parse(rep("\"1\"", 1)) }
function float_parse_repeated() -> float { float.parse(rep("12", 3)) }
