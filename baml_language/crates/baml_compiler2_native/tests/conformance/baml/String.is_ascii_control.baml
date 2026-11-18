function string_is_ascii_control_0() -> bool { rep("\u{0}\u{1f}\u{7f}", 2).is_ascii_control() }
function string_is_ascii_control_1() -> bool { rep("\u{0}a", 2).is_ascii_control() }
function string_is_ascii_control_2() -> bool { rep("\u{1}\u{85}", 2).is_ascii_control() }
function string_is_ascii_control_3() -> bool { rep("x", 0).is_ascii_control() }
