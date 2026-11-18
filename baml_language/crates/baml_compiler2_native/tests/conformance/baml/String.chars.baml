function string_chars_ascii() -> string {
  let c = rep("abc", 2).chars();
  `${c.length()}:${c[0]}|${c[2]}|${c[5]}`
}
function string_chars_wide() -> string {
  let c = rep("é1😀", 2).chars();
  `${c.length()}:${c[0]}|${c[2]}|${c[3]}|${c[2].length()}|${c[2].byte_length()}`
}
function string_chars_empty() -> int { rep("a", 0).chars().length() }
function string_chars_many() -> int { rep("xy😀", 400).chars().length() }
