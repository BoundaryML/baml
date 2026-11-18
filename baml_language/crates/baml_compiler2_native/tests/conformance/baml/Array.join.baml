function array_join_strings() -> string { words(3).join(rep("-", 2)) }
function array_join_ints() -> string { ints(4).join(", ") }
function array_join_floats() -> string { floats(3).join("|") }
function array_join_bools() -> string {
  let xs: bool[] = [];
  for (let i = 0; i < 3; i += 1) { xs.push(i == 1); };
  xs.join(" ")
}
function array_join_empty_and_one() -> string { `[${ints(0).join(",")}][${words(1).join(",")}]` }
