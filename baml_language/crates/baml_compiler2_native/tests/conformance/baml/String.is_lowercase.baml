function string_is_lowercase_0() -> bool { rep("abc", 2).is_lowercase() }
function string_is_lowercase_1() -> bool { rep("abC", 2).is_lowercase() }
function string_is_lowercase_2() -> bool { rep("éß", 2).is_lowercase() }
function string_is_lowercase_3() -> bool { rep("éÉ", 2).is_lowercase() }
function string_is_lowercase_4() -> bool { rep("a1", 2).is_lowercase() }
function string_is_lowercase_5() -> bool { rep("x", 0).is_lowercase() }
