function string_is_ascii_0() -> bool { rep("abc \u{7f}", 2).is_ascii() }
function string_is_ascii_1() -> bool { rep("abcé", 2).is_ascii() }
function string_is_ascii_2() -> bool { rep("😀", 2).is_ascii() }
function string_is_ascii_3() -> bool { rep("x", 0).is_ascii() }
