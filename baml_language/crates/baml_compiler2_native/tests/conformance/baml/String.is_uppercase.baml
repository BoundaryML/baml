function string_is_uppercase_0() -> bool { rep("ABC", 2).is_uppercase() }
function string_is_uppercase_1() -> bool { rep("ABc", 2).is_uppercase() }
function string_is_uppercase_2() -> bool { rep("ÉΩ", 2).is_uppercase() }
function string_is_uppercase_3() -> bool { rep("Éé", 2).is_uppercase() }
function string_is_uppercase_4() -> bool { rep("A1", 2).is_uppercase() }
function string_is_uppercase_5() -> bool { rep("x", 0).is_uppercase() }
