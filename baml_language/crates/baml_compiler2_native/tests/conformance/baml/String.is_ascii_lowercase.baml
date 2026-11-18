function string_is_ascii_lowercase_0() -> bool { rep("abc", 2).is_ascii_lowercase() }
function string_is_ascii_lowercase_1() -> bool { rep("abC", 2).is_ascii_lowercase() }
function string_is_ascii_lowercase_2() -> bool { rep("abé", 2).is_ascii_lowercase() }
function string_is_ascii_lowercase_3() -> bool { rep("x", 0).is_ascii_lowercase() }
