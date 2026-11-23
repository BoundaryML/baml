// `int[].sum()`, `float[].sum()` and `float[].median()`.

function sum_ints(n: int) -> int[] {
  let out: int[] = [];
  for (let i = 0; i < n; i += 1) { out.push(i * 3 - 4); };
  out
}
function sum_floats(n: int) -> float[] {
  let out: float[] = [];
  for (let i = 0; i < n; i += 1) { out.push(halves(n - i) * halves(i + 1)); };
  out
}

function sum_int() -> int { sum_ints(100).sum() }
function sum_int_empty() -> int { sum_ints(0).sum() }
function sum_int_one() -> int { sum_ints(1).sum() }
// native prints: -9223372036854775808
function sum_int_overflows() -> int {
  let values = sum_ints(0);
  values.push(4611686018427387903);
  values.push(4611686018427387903);
  values.push(2);
  values.sum()
}
function sum_float() -> float { sum_floats(50).sum() }
function sum_float_empty() -> float { sum_floats(0).sum() }
function sum_float_nan() -> bool {
  let values = sum_floats(3);
  values.push(float.nan());
  values.sum().is_nan()
}
function sum_float_infinite() -> float {
  let values = sum_floats(3);
  values.push(float.inf());
  values.sum()
}

function median_odd() -> float { sum_floats(7).median() }
function median_even() -> float { sum_floats(8).median() }
function median_one() -> float { sum_floats(1).median() }
function median_empty() -> float { sum_floats(0).median() }
function median_empty_caught() -> string {
  sum_floats(0).median() catch (e) {
    baml.errors.InvalidArgument => e.message,
  };
  "no throw"
}
// NaN ranks above every number.
function median_with_nan() -> float {
  let values = sum_floats(2);
  values.push(float.nan());
  values.median()
}
function median_leaves_the_array() -> string {
  let values = sum_floats(5);
  let middle = values.median();
  `${middle} ${values[0]} ${values[4]}`
}
