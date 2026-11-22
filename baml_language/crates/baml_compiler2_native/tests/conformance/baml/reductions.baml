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
// A running total that leaves the range of an `int` panics, and says at
// which element.
function sum_int_overflows() -> int {
  let values = sum_ints(0);
  values.push(4611686018427387903);
  values.push(4611686018427387903);
  values.push(2);
  values.sum()
}
function sum_int_overflows_by_one() -> int {
  let values = sum_ints(3);
  values.push(4611686018427387903);
  values.push(4);
  values.sum()
}
function sum_int_underflows() -> int {
  let values = sum_ints(0);
  values.push(0 - 4611686018427387903);
  values.push(-1);
  values.push(-1);
  values.sum()
}
// The totals at the two ends of the range are sums.
function sum_int_ends() -> string {
  let most = sum_ints(0);
  most.push(4611686018427387900);
  most.push(3);
  let least = sum_ints(0);
  least.push(0 - 4611686018427387903);
  least.push(-1);
  `${most.sum()} ${least.sum()}`
}
function sum_int_overflow_caught() -> string {
  let values = sum_ints(0);
  values.push(4611686018427387903);
  values.push(1);
  let total = values.sum() catch (e) {
    baml.panics.IntegerOverflow => { return e.message; },
  };
  `${total}`
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
