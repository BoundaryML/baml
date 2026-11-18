function array_slice_middle() -> string { show(ints(6).slice(1, 4)) }
function array_slice_negative_bounds() -> string { show_words(words(5).slice(-3, -1)) }
function array_slice_clamped() -> string { `${show(ints(3).slice(-9, 9))}|${show(ints(3).slice(2, 1))}` }
function array_slice_is_a_copy() -> string {
  let xs = ints(3);
  let ys = xs.slice(0, 3);
  ys.push(99);
  `${show(xs)}|${show(ys)}`
}
