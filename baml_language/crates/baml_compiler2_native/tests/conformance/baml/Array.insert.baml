function array_insert_middle() -> string {
  let xs = ints(3);
  let n = xs.insert(count(5), 1);
  `${n}|${show(xs)}`
}
function array_insert_at_end() -> string {
  let xs = words(4);
  xs.insert(rep("x", 2), 4);
  xs.insert(rep("y", 2), -1);
  show_words(xs)
}
function array_insert_out_of_range() -> int { ints(2).insert(1, 3) }
function array_insert_before_start() -> int { ints(2).insert(1, -3) }
