function string_is_alphanumeric_0() -> bool { rep("abc123", 2).is_alphanumeric() }
function string_is_alphanumeric_1() -> bool { rep("ab_1", 2).is_alphanumeric() }
function string_is_alphanumeric_2() -> bool { rep("é٣", 2).is_alphanumeric() }
function string_is_alphanumeric_3() -> bool { rep("é ", 2).is_alphanumeric() }
function string_is_alphanumeric_4() -> bool { rep("x", 0).is_alphanumeric() }
