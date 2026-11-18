function string_split_parts() -> string {
  let p = rep("a,bb;", 3).split(",");
  `${p.length()}:${p[0]}|${p[1]}|${p[3]}`
}
function string_split_long_delimiter() -> string {
  let p = rep("one--two--", 2).split("--");
  `${p.length()}:${p[0]}|${p[1]}|${p[4]}|`
}
function string_split_no_match() -> string {
  let p = rep("abc", 2).split("zz");
  `${p.length()}:${p[0]}`
}
function string_split_empty_string() -> string {
  let p = rep("abc", 0).split(",");
  `${p.length()}:${p[0]}|`
}
function string_split_into_characters() -> string {
  let p = rep("é1😀", 2).split("");
  `${p.length()}:${p[0]}|${p[2]}|${p[5]}`
}
function string_split_many() -> int { rep("x y ", 500).split(" ").length() }
