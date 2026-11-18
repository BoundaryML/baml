function string_starts_with_yes() -> bool { rep("abc", 2).starts_with("abca") }
function string_starts_with_no() -> bool { rep("abc", 2).starts_with("abcb") }
function string_starts_with_longer() -> bool { rep("abc", 1).starts_with(rep("abc", 2)) }
function string_starts_with_empty() -> bool { rep("abc", 1).starts_with(rep("", 1)) }
function string_starts_with_both_empty() -> bool { rep("", 1).starts_with(rep("", 1)) }
function string_starts_with_wide() -> bool { rep("é1😀", 2).starts_with("é1😀é") }
function string_starts_with_whole() -> bool { rep("é1", 2).starts_with(rep("é1", 2)) }
