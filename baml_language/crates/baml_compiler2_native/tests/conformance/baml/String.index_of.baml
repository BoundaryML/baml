function string_index_of_found() -> int { rep("hello world ", 2).index_of("world") ?? -1 }
function string_index_of_missing() -> int { rep("hello", 2).index_of("z") ?? -1 }
function string_index_of_missing_long() -> int { rep("hello", 2).index_of("lloz") ?? -1 }
function string_index_of_one_byte() -> int { rep("hello", 2).index_of("o") ?? -1 }
function string_index_of_wide() -> int { rep("é😀hello", 2).index_of("h") ?? -1 }
function string_index_of_wide_search() -> int { rep("aé😀b", 2).index_of("😀b") ?? -1 }
function string_index_of_wide_late() -> int { rep("é😀hello", 2).index_of("lloé😀hell") ?? -1 }
function string_index_of_empty_search() -> int { rep("é😀", 2).index_of(rep("", 1)) ?? -1 }
function string_index_of_empty_string() -> int { rep("", 1).index_of("a") ?? -1 }
function string_index_of_both_empty() -> int { rep("", 1).index_of(rep("", 1)) ?? -1 }
function string_index_of_is_null() -> bool { rep("abc", 1).index_of("d") == null }
function string_index_of_zero_is_not_null() -> bool { rep("abc", 1).index_of("a") == null }
