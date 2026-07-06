// Tests for `Array.sum()` (the `Summable` interface in stdlib `containers.baml`).
//
// `sum()` is a method — the canonical surface for array summation — implemented
// for `int[]` (returning `int`, native `_sum_int`) and `float[]` (returning
// `float`, delegating to `baml.math.sum`). `int` and `float` are distinct types
// with no widening, so each impl stays in its own number domain. Results are
// compared with `baml.deep_equals`, which is type-aware: an `int` `6` does not
// deep-equal a `float` `6.0`, so these also pin the return type.
//
// The overflow / boundary helpers take runtime args so the array is built (and
// summed) at runtime rather than being constant-folded away.

// ─── int[].sum() ──────────────────────────────────────────────────────────────

test "array_sum_int_basic" {
  assert.is_true(baml.deep_equals([1, 2, 3].sum(), 6))
}

test "array_sum_int_single" {
  assert.is_true(baml.deep_equals([42].sum(), 42))
}

// Summation of no elements is well-defined: the empty array sums to `0`.
test "array_sum_int_empty_is_zero" {
  let xs: int[] = [];
  assert.is_true(baml.deep_equals(xs.sum(), 0))
}

test "array_sum_int_negatives" {
  assert.is_true(baml.deep_equals([-5, 10, -3].sum(), 2))
}

// Mixed magnitudes add exactly (no float rounding in the int domain):
// 1000000 + 20 + 300 == 1000320.
test "array_sum_int_mixed_magnitudes" {
  assert.is_true(baml.deep_equals([1000000, 20, 300].sum(), 1000320))
}

// A running total that leaves the `int` range raises a catchable
// `baml.panics.IntegerOverflow`, exactly like the `+` it stands in for.
function sum_int_overflow_(a: int, b: int) -> int {
  [a, b].sum() catch (e) { baml.panics.IntegerOverflow => -1 }
}

test "array_sum_int_overflow_throws" {
  assert.is_true(baml.deep_equals(sum_int_overflow_(int.max_value(), 1), -1))
}

// Summing right up to the boundary must NOT spuriously overflow:
// (MAX - 1) + 1 == MAX.
function sum_int_two_(a: int, b: int) -> int {
  [a, b].sum()
}

test "array_sum_int_boundary_no_overflow" {
  assert.is_true(baml.deep_equals(sum_int_two_(int.max_value() - 1, 1), int.max_value()))
}

// ─── float[].sum() ────────────────────────────────────────────────────────────

test "array_sum_float_basic" {
  assert.is_true(baml.deep_equals([1.0, 2.0, 3.0].sum(), 6.0))
}

test "array_sum_float_single" {
  assert.is_true(baml.deep_equals([42.0].sum(), 42.0))
}

test "array_sum_float_empty_is_zero" {
  let xs: float[] = [];
  assert.is_true(baml.deep_equals(xs.sum(), 0.0))
}

// Mixed magnitudes and signs: -1.5 + 2.5 + 1.0 == 2.0.
test "array_sum_float_mixed" {
  assert.is_true(baml.deep_equals([-1.5, 2.5, 1.0].sum(), 2.0))
}
