function string_is_control_0() -> bool { rep("\u{0}\u{1f}\u{7f}", 2).is_control() }
function string_is_control_1() -> bool { rep("\u{0}a", 2).is_control() }
function string_is_control_2() -> bool { rep("\u{85}\u{9f}", 2).is_control() }
function string_is_control_3() -> bool { rep("\u{85}é", 2).is_control() }
function string_is_control_4() -> bool { rep("x", 0).is_control() }
