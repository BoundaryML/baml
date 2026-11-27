function array_shift_ints() -> string {
  let xs = ints(4);
  let first = xs.shift() ?? -1;
  `${first}|${show(xs)}`
}
function array_shift_empty() -> int { ints(0).shift() ?? -1 }
function array_shift_strings() -> string {
  let xs = words(3);
  let first = xs.shift() ?? "none";
  `${first}|${show_words(xs)}`
}
