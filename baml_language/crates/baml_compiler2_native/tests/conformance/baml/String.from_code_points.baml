function string_from_code_points_ascii() -> string { string.from_code_points(rep("hello", 2).to_code_points()) }
function string_from_code_points_wide() -> string {
  let s = string.from_code_points(rep("é1😀", 2).to_code_points());
  `${s}|${s.length()}|${s.byte_length()}`
}
function string_from_code_points_empty() -> string { `[${string.from_code_points(rep("", 1).to_code_points())}]` }
function string_from_code_points_built() -> string {
  let points = rep("ab", 1).to_code_points();
  points.push(233);
  points.push(128512);
  points.push(1114111);
  let s = string.from_code_points(points);
  `${s.length()}|${s.byte_length()}|${s.slice(0, 4)}`
}
function string_from_code_points_surrogate() -> string {
  let points = rep("ab", 1).to_code_points();
  points.push(55296);
  string.from_code_points(points)
}
function string_from_code_points_too_large() -> string {
  let points = rep("ab", 1).to_code_points();
  points.push(1114112);
  string.from_code_points(points)
}
function string_from_code_points_negative() -> string {
  let points = rep("", 1).to_code_points();
  points.push(-1);
  string.from_code_points(points)
}
