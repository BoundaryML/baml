function string_last_index_of_found() -> int { rep("hello world ", 2).last_index_of("world") ?? -1 }
function string_last_index_of_missing() -> int { rep("hello", 2).last_index_of("z") ?? -1 }
function string_last_index_of_one_byte() -> int { rep("hello", 2).last_index_of("h") ?? -1 }
function string_last_index_of_wide() -> int { rep("é😀hello", 2).last_index_of("h") ?? -1 }
function string_last_index_of_wide_early() -> int { rep("é😀hello", 3).last_index_of("é😀helloé😀helloé") ?? -1 }
function string_last_index_of_wide_search() -> int { rep("aé😀b", 2).last_index_of("é😀") ?? -1 }
function string_last_index_of_empty_search() -> int { rep("é😀", 2).last_index_of(rep("", 1)) ?? -1 }
function string_last_index_of_empty_string() -> int { rep("", 1).last_index_of("a") ?? -1 }
function string_last_index_of_is_null() -> bool { rep("abc", 1).last_index_of("d") == null }
