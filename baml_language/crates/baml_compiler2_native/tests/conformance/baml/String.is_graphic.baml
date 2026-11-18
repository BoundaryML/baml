function string_is_graphic_0() -> bool { rep("abc!~", 2).is_graphic() }
function string_is_graphic_1() -> bool { rep("ab c", 2).is_graphic() }
function string_is_graphic_2() -> bool { rep("a\u{7f}", 2).is_graphic() }
function string_is_graphic_3() -> bool { rep("é😀", 2).is_graphic() }
function string_is_graphic_4() -> bool { rep("é\u{3000}", 2).is_graphic() }
function string_is_graphic_5() -> bool { rep("é\u{85}", 2).is_graphic() }
function string_is_graphic_6() -> bool { rep("x", 0).is_graphic() }
