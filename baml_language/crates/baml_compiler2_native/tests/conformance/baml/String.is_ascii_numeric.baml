function string_is_ascii_numeric_0() -> bool { rep("0129", 2).is_ascii_numeric() }
function string_is_ascii_numeric_1() -> bool { rep("12a", 2).is_ascii_numeric() }
function string_is_ascii_numeric_2() -> bool { rep("1٣", 2).is_ascii_numeric() }
function string_is_ascii_numeric_3() -> bool { rep("x", 0).is_ascii_numeric() }
