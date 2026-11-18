function string_slice_middle() -> string { rep("abcdef", 2).slice(4, 9) }
function string_slice_from_end() -> string { rep("abcdef", 2).slice(-3, -1) }
function string_slice_clamps() -> string { rep("abc", 2).slice(-100, 100) }
function string_slice_end_before_start() -> string { rep("abc", 2).slice(4, 2) }
function string_slice_non_ascii() -> string { rep("é1😀", 2).slice(1, 5) }
function string_slice_whole_non_ascii() -> string { rep("é1😀", 2).slice(0, 6) }
