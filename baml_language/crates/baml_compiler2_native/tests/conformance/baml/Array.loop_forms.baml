function array_map_ints() -> string { show(ints(4).map((x) -> { x + 1 })) }
function array_map_to_strings() -> string { show_words(ints(3).map((x) -> { rep("m", x / 10 + 1) })) }
function array_map_objects() -> string { show(people(3).map((p) -> { p.rank * 2 })) }
function array_filter_ints() -> string { show(ints(6).filter((x) -> { x % 20 == 0 })) }
function array_filter_objects() -> string { ranks(people(5).filter((p) -> { p.rank > 2 })) }
function array_some_every() -> string {
  let xs = ints(4);
  `${xs.some((x) -> { x == 20 })}${xs.some((x) -> { x == 21 })}${xs.every((x) -> { x >= 0 })}${xs.every((x) -> { x > 0 })}`
}
function array_find() -> string {
  let found = people(4).find((p) -> { p.rank == 2 });
  let missing = ints(3).find((x) -> { x > 99 }) ?? -1;
  let name = if (found == null) { "none" } else { found.name };
  `${name}|${missing}|${ints(5).find_index((x) -> { x == 30 }) ?? -1}`
}
function array_reduce() -> int { ints(5).reduce((sum, x) -> { sum + x }, count(1)) }
function pick_step(n: int) -> (int) -> int throws never {
  if (count(n) > 1) { (x) -> { x * 3 } } else { (x) -> { x + 1 } }
}
function array_map_with_a_function_value() -> string {
  let f = pick_step(2);
  let g = pick_step(1);
  `${show(ints(3).map(f))}|${show(ints(3).map(g))}|${show(ints(4).filter((x) -> { f(x) > 30 }))}`
}
