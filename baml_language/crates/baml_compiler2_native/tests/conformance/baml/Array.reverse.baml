function array_reverse_ints() -> string {
  let xs = ints(4);
  `${show(xs.reverse())}|${show(xs)}`
}
function array_reverse_objects() -> string { ranks(people(3).reverse()) }
function array_reverse_empty() -> string { show(ints(0).reverse()) }
