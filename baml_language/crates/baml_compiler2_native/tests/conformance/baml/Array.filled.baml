function array_filled_ints() -> string { show(baml.Array.filled<int>(count(3), count(7))) }
function array_filled_strings() -> string {
  let xs = baml.Array.filled<string>(count(2), rep("ab", 2));
  xs.push("c");
  show_words(xs)
}
function array_filled_none() -> string { `${show(baml.Array.filled<int>(count(0), 1))}|${show(baml.Array.filled<int>(-count(2), 1))}` }
