function array_unshift_ints() -> string {
  let xs = ints(2);
  let n = xs.unshift(count(5));
  `${n}|${show(xs)}`
}
function array_unshift_grows() -> string {
  let xs: string[] = [];
  for (let i = 0; i < 9; i += 1) { xs.unshift(rep("u", i + 1)); };
  show_words(xs)
}
function array_unshift_objects() -> string {
  let xs = people(4);
  xs.unshift(Named { name: rep("z", 2), rank: 40 });
  ranks(xs)
}
