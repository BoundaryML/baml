function string_trim_start_both() -> string { `[${rep("  \t ab c  ", 1).trim_start()}]` }
function string_trim_start_none() -> string { `[${rep("ab ", 2).trim_start()}]` }
function string_trim_start_all() -> string { `[${rep(" \n", 3).trim_start()}]` }
function string_trim_start_wide() -> string {
  let t = rep("\u{3000}\u{a0}é😀 ", 1).trim_start();
  `[${t}]${t.length()}`
}
function string_trim_start_empty() -> string { `[${rep(" ", 0).trim_start()}]` }
