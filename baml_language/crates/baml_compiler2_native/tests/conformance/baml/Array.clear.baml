function array_clear_then_push() -> string {
  let xs = people(5);
  xs.clear();
  xs.push(Named { name: "new", rank: 9 });
  ranks(xs)
}
function array_clear_empty() -> string {
  let xs = ints(0);
  xs.clear();
  show(xs)
}
