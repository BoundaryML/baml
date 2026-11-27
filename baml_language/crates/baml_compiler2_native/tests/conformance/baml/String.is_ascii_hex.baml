function string_is_ascii_hex_0() -> bool { rep("09afAF", 2).is_ascii_hex() }
function string_is_ascii_hex_1() -> bool { rep("0g", 2).is_ascii_hex() }
function string_is_ascii_hex_2() -> bool { rep("aé", 2).is_ascii_hex() }
function string_is_ascii_hex_3() -> bool { rep("x", 0).is_ascii_hex() }
