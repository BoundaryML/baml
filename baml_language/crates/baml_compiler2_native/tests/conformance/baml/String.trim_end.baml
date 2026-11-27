function string_trim_end_both() -> string { `[${rep("  ab c \t ", 1).trim_end()}]` }
function string_trim_end_none() -> string { `[${rep(" ab", 2).trim_end()}]` }
function string_trim_end_all() -> string { `[${rep(" \n", 3).trim_end()}]` }
function string_trim_end_wide() -> string {
  let t = rep(" é😀\u{3000}\u{a0}", 1).trim_end();
  `[${t}]${t.length()}`
}
function string_trim_end_empty() -> string { `[${rep(" ", 0).trim_end()}]` }
