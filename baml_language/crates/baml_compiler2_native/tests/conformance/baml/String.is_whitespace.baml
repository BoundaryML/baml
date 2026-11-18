function string_is_whitespace_0() -> bool { rep(" \t\n\r", 2).is_whitespace() }
function string_is_whitespace_1() -> bool { rep(" a", 2).is_whitespace() }
function string_is_whitespace_2() -> bool { rep("\u{b}\u{c}", 2).is_whitespace() }
function string_is_whitespace_3() -> bool { rep("\u{3000}\u{a0} ", 2).is_whitespace() }
function string_is_whitespace_4() -> bool { rep("\u{3000}é", 2).is_whitespace() }
function string_is_whitespace_5() -> bool { rep("x", 0).is_whitespace() }
