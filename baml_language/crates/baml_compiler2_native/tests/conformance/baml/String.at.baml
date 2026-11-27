function string_at_first() -> string { rep("hello", 2).at(0) ?? "<null>" }
function string_at_last() -> string { rep("hello", 2).at(9) ?? "<null>" }
function string_at_past_end() -> string { rep("hello", 2).at(10) ?? "<null>" }
function string_at_negative() -> string { rep("hello", 2).at(-1) ?? "<null>" }
function string_at_negative_first() -> string { rep("hello", 2).at(-10) ?? "<null>" }
function string_at_negative_past() -> string { rep("hello", 2).at(-11) ?? "<null>" }
function string_at_wide() -> string {
  let s = rep("é1😀", 2);
  let c = s.at(2) ?? "<null>";
  `${c}${s.at(3) ?? "<null>"}${s.at(-1) ?? "<null>"}${s.at(6) ?? "<null>"}|${c.length()}|${c.byte_length()}`
}
function string_at_single() -> string { rep("😀", 1).at(0) ?? "<null>" }
function string_at_empty() -> string { rep("", 1).at(0) ?? "<null>" }
function string_at_is_null() -> bool { rep("abc", 1).at(3) == null }
