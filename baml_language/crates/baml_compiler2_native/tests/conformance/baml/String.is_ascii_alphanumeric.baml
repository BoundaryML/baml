function string_is_ascii_alphanumeric_0() -> bool { rep("abc123", 2).is_ascii_alphanumeric() }
function string_is_ascii_alphanumeric_1() -> bool { rep("ab_1", 2).is_ascii_alphanumeric() }
function string_is_ascii_alphanumeric_2() -> bool { rep("a1é", 2).is_ascii_alphanumeric() }
function string_is_ascii_alphanumeric_3() -> bool { rep("x", 0).is_ascii_alphanumeric() }
