function array_at_in_range() -> int { (ints(4).at(1) ?? -1) + (ints(4).at(-1) ?? -1) }
function array_at_out_of_range() -> int { (ints(4).at(4) ?? -1) + (ints(4).at(-5) ?? -2) }
function array_at_strings() -> string { `${words(3).at(-3) ?? "none"}|${words(3).at(9) ?? "none"}` }
