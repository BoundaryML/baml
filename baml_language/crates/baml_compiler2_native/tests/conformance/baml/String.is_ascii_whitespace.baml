function string_is_ascii_whitespace_0() -> bool { rep(" \t\n\r\u{c}", 2).is_ascii_whitespace() }
function string_is_ascii_whitespace_1() -> bool { rep(" a", 2).is_ascii_whitespace() }
function string_is_ascii_whitespace_2() -> bool { rep(" \u{b}", 2).is_ascii_whitespace() }
function string_is_ascii_whitespace_3() -> bool { rep(" \u{3000}", 2).is_ascii_whitespace() }
function string_is_ascii_whitespace_4() -> bool { rep("x", 0).is_ascii_whitespace() }
