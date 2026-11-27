function array_join_strings() -> string { words(3).join(rep("-", 2)) }
function array_join_ints() -> string { ints(4).join(", ") }
function array_join_floats() -> string { floats(3).join("|") }
function array_join_bools() -> string {
  let xs: bool[] = [];
  for (let i = 0; i < 3; i += 1) { xs.push(i == 1); };
  xs.join(" ")
}
function array_join_empty_and_one() -> string { `[${ints(0).join(",")}][${words(1).join(",")}]` }
function array_join_past_ascii() -> string {
  let s = words(3).join(rep("→é", 1));
  `${s.length()}:${s}`
}
function array_join_int_widths() -> string {
  let xs = [0, 9, 10, 99, 100, 101, 12345, -1, -10, -99999, 1000000007, -9007199254740991, 9007199254740991];
  xs.push(xs.length());
  xs.join(",")
}
function array_join_int_to_string_widths() -> string {
  let xs = [0, 9, 10, 99, 100, 101, 12345, -1, -10, -99999, 1000000007, -9007199254740991, 9007199254740991];
  let out = "";
  for (let i = 0; i < xs.length(); i += 1) { out = out + xs[i].to_string() + `<${xs[i]}>`; };
  out
}
// An infinity is spelled out inside a string, as a template spells it.
function array_join_infinite_floats() -> string {
  let values: float[] = [float.inf(), 0.0 - float.inf(), float.nan(), halves(3)];
  values.join(" ")
}
