function string_repeat_some() -> string { rep("ab", 1).repeat(3) }
function string_repeat_once() -> string { rep("ab", 2).repeat(1) }
function string_repeat_zero() -> string { `[${rep("ab", 1).repeat(0)}]` }
function string_repeat_negative() -> string { `[${rep("ab", 1).repeat(-4)}]` }
function string_repeat_empty() -> string { `[${rep("", 1).repeat(5)}]` }
function string_repeat_wide() -> string {
  let r = rep("é😀", 1).repeat(3);
  `${r}|${r.length()}|${r.byte_length()}`
}
function string_repeat_many() -> int { rep("abc", 1).repeat(10000).length() }
