function string_is_numeric_0() -> bool { rep("0129", 2).is_numeric() }
function string_is_numeric_1() -> bool { rep("12a", 2).is_numeric() }
function string_is_numeric_2() -> bool { rep("٣½", 2).is_numeric() }
function string_is_numeric_3() -> bool { rep("😀", 2).is_numeric() }
function string_is_numeric_4() -> bool { rep("x", 0).is_numeric() }
