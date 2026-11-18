function array_concat_ints() -> string { show(ints(2).concat(ints(3))) }
function array_concat_strings() -> string { show_words(words(2).concat(words(1))) }
function array_concat_empty() -> string { `${show(ints(0).concat(ints(0)))}|${show(ints(2).concat(ints(0)))}` }
function array_concat_with_itself() -> string {
  let xs = people(2);
  ranks(xs.concat(xs))
}
