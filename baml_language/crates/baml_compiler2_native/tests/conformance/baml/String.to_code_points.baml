function string_to_code_points_mixed() -> string {
  let c = rep("aé😀", 2).to_code_points();
  `${c.length()}:${c[0]},${c[1]},${c[2]},${c[5]}`
}
function string_to_code_points_empty() -> int { rep("a", 0).to_code_points().length() }
