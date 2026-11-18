function array_remove_at_middle() -> string {
  let xs = ints(5);
  let removed = xs.remove_at(2) ?? -1;
  `${removed}|${show(xs)}`
}
function array_remove_at_from_end() -> string {
  let xs = words(4);
  let removed = xs.remove_at(-1) ?? "none";
  `${removed}|${show_words(xs)}`
}
function array_remove_at_out_of_range() -> string {
  let xs = ints(3);
  let a = xs.remove_at(3) ?? -1;
  let b = xs.remove_at(-4) ?? -1;
  `${a},${b}|${show(xs)}`
}
