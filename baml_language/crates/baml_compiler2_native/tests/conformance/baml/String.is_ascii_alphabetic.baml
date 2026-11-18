function string_is_ascii_alphabetic_0() -> bool { rep("abcXYZ", 2).is_ascii_alphabetic() }
function string_is_ascii_alphabetic_1() -> bool { rep("ab1", 2).is_ascii_alphabetic() }
function string_is_ascii_alphabetic_2() -> bool { rep("abé", 2).is_ascii_alphabetic() }
function string_is_ascii_alphabetic_3() -> bool { rep("x", 0).is_ascii_alphabetic() }
