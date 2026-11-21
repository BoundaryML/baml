// `baml._rust_sort` and its guard, which `sort()` is written on: in place by
// the order of the elements, for `int`, `float`, `string` and `bigint`.

function sort_scrambled(n: int) -> int[] {
  let out: int[] = [];
  for (let i = 0; i < n; i += 1) { out.push((i * 37 + 11) % 23 - 9); };
  out
}
function sort_ints() -> string { baml._rust_sort(sort_scrambled(40)).join(" ") }
function sort_ints_empty() -> int { baml._rust_sort(sort_scrambled(0)).length() }
function sort_ints_one() -> string { baml._rust_sort(sort_scrambled(1)).join(" ") }
// The array itself is sorted, and returned.
function sort_ints_in_place() -> string {
  let values = sort_scrambled(9);
  let sorted = baml._rust_sort(values);
  sorted.push(100);
  values.join(" ")
}
// NaN ranks above every number, and the two zeros are equal.
function sort_floats() -> string {
  let values: float[] = [];
  for (let i = 0; i < 12; i += 1) { values.push(halves(i * 7 % 5) - halves(3)); };
  values.push(float.nan());
  values.push(0.0 - halves(0));
  values.push(float.inf());
  values.push(0.0 - float.inf());
  baml._rust_sort(values).join(" ")
}
function sort_strings() -> string {
  let values: string[] = [];
  for (let i = 0; i < 12; i += 1) { values.push(rep("b", i * 5 % 4) + rep("é", i % 3) + rep("a", i % 2)); };
  baml._rust_sort(values).join(",")
}
function sort_long_strings() -> string {
  let values: string[] = [];
  for (let i = 0; i < 6; i += 1) { values.push(rep("xy", 30 - i * 3).slice(i % 3, 60)); };
  baml._rust_sort(values).map((s: string) -> int { s.length() }).join(" ")
}
function sort_bigints() -> string {
  let values: bigint[] = [];
  for (let i = 0; i < 10; i += 1) { values.push(big_two_to(i * 37 % 90) - big_two_to(70)); };
  let out = "";
  for (let value in baml._rust_sort(values)) { out = `${out}${value} `; };
  out
}
// The guard reads the type of the elements, so an empty array passes too.
function sort_guard() -> string {
  let texts: string[] = [];
  let bigs: bigint[] = [big(1)];
  `${baml._is_primitive_array(sort_scrambled(3))} ${baml._is_primitive_array(texts)} ${baml._is_primitive_array(bigs)} ${baml._is_primitive_array([halves(1)])}`
}
