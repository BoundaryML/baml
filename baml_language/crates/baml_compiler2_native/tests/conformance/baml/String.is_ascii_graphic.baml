function string_is_ascii_graphic_0() -> bool { rep("abc!~", 2).is_ascii_graphic() }
function string_is_ascii_graphic_1() -> bool { rep("ab c", 2).is_ascii_graphic() }
function string_is_ascii_graphic_2() -> bool { rep("abé", 2).is_ascii_graphic() }
function string_is_ascii_graphic_3() -> bool { rep("x", 0).is_ascii_graphic() }
