function string_byte_length_ascii() -> int { rep("abc", 3).byte_length() }
function string_byte_length_wide() -> int { rep("é1😀", 2).byte_length() }
function string_byte_length_empty() -> int { rep("x", 0).byte_length() }
