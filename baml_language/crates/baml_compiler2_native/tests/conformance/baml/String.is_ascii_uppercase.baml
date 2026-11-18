function string_is_ascii_uppercase_0() -> bool { rep("ABC", 2).is_ascii_uppercase() }
function string_is_ascii_uppercase_1() -> bool { rep("ABc", 2).is_ascii_uppercase() }
function string_is_ascii_uppercase_2() -> bool { rep("ABÉ", 2).is_ascii_uppercase() }
function string_is_ascii_uppercase_3() -> bool { rep("x", 0).is_ascii_uppercase() }
