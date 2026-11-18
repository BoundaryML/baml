function string_ends_with_yes() -> bool { rep("abc", 2).ends_with("cabc") }
function string_ends_with_no() -> bool { rep("abc", 2).ends_with("babc") }
function string_ends_with_longer() -> bool { rep("abc", 1).ends_with(rep("abc", 2)) }
function string_ends_with_empty() -> bool { rep("abc", 1).ends_with(rep("", 1)) }
function string_ends_with_both_empty() -> bool { rep("", 1).ends_with(rep("", 1)) }
function string_ends_with_wide() -> bool { rep("é1😀", 2).ends_with("😀é1😀") }
function string_ends_with_whole() -> bool { rep("é1", 2).ends_with(rep("é1", 2)) }
