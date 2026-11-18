function array_pop_ints() -> string {
  let xs = ints(3);
  let last = xs.pop() ?? -1;
  `${last}|${show(xs)}`
}
function array_pop_until_empty() -> string {
  let xs = ints(2);
  let a = xs.pop() ?? -1;
  let b = xs.pop() ?? -1;
  let c = xs.pop() ?? -1;
  xs.push(7);
  `${a},${b},${c}|${show(xs)}`
}
function array_pop_objects() -> string {
  let xs = people(3);
  let last = xs.pop();
  let rank = if (last == null) { -1 } else { last.rank };
  `${rank}|${ranks(xs)}`
}
function array_pop_floats() -> float { floats(4).pop() ?? -1.0 }
