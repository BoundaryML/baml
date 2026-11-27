function array_splice_replace() -> string {
  let xs = ints(5);
  xs.splice(1, 2, [7, 8, 9]);
  show(xs)
}
function array_splice_remove() -> string {
  let xs = words(5);
  xs.splice(-2, 9, []);
  show_words(xs)
}
function array_splice_insert_objects() -> string {
  let xs = people(2);
  xs.splice(1, 0, people(6));
  ranks(xs)
}
function array_splice_with_itself() -> string {
  let xs = ints(3);
  xs.splice(1, 1, xs);
  show(xs)
}
function array_splice_start_out_of_range() -> null { ints(2).splice(3, 0, []) }
function array_splice_negative_count() -> null { ints(2).splice(0, -count(1), []) }
