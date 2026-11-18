function string_is_alphabetic_0() -> bool { rep("abcXYZ", 2).is_alphabetic() }
function string_is_alphabetic_1() -> bool { rep("ab1", 2).is_alphabetic() }
function string_is_alphabetic_2() -> bool { rep("éßΩ", 2).is_alphabetic() }
function string_is_alphabetic_3() -> bool { rep("é1", 2).is_alphabetic() }
function string_is_alphabetic_4() -> bool { rep("x", 0).is_alphabetic() }
