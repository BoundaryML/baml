// Tests for the `baml.math` aggregation builtins: sum / mean / median.
//
// These are native VM functions (`$rust_function`, `//baml:vm`), so the coverage
// here exercises both the happy path and the empty-array error path. Exact
// (integer-valued or terminating-binary) float results are compared with
// `baml.deep_equals`; the empty-array cases assert the `InvalidArgument` throw.

// ─── sum ────────────────────────────────────────────────────────────────────

test "math_sum_basic" {
  assert.is_true(baml.deep_equals(baml.math.sum([1.0, 2.0, 3.0, 4.0]), 10.0))
}

test "math_sum_single" {
  assert.is_true(baml.deep_equals(baml.math.sum([42.0]), 42.0))
}

test "math_sum_negatives" {
  assert.is_true(baml.deep_equals(baml.math.sum([-1.0, 2.0, -3.0]), -2.0))
}

// The empty array sums to 0.0 (the fold's initial accumulator) rather than
// throwing — summation of no elements is well-defined.
test "math_sum_empty_is_zero" {
  assert.is_true(baml.deep_equals(baml.math.sum([]), 0.0))
}

// ─── mean ───────────────────────────────────────────────────────────────────

test "math_mean_basic" {
  assert.is_true(baml.deep_equals(baml.math.mean([1.0, 2.0, 3.0, 4.0]), 2.5))
}

test "math_mean_single" {
  assert.is_true(baml.deep_equals(baml.math.mean([5.0]), 5.0))
}

test "math_mean_empty_throws" {
  {
    baml.math.mean([]);
    baml.sys.panic("expected baml.math.mean([]) to throw")
  } catch (e) {
    baml.errors.InvalidArgument => null
  }
}

// ─── median ───────────────────────────────────────────────────────────────────

// Odd count: the single middle element after sorting.
test "math_median_odd" {
  assert.is_true(baml.deep_equals(baml.math.median([3.0, 1.0, 2.0]), 2.0))
}

// Even count: the arithmetic mean of the two middle elements after sorting.
test "math_median_even" {
  assert.is_true(baml.deep_equals(baml.math.median([1.0, 2.0, 3.0, 4.0]), 2.5))
}

test "math_median_unsorted_input" {
  assert.is_true(baml.deep_equals(baml.math.median([5.0, 1.0, 4.0, 2.0, 3.0]), 3.0))
}

// `median` sorts a copy, so the caller's array is observably untouched.
test "math_median_does_not_mutate_input" {
  let xs = [3.0, 1.0, 2.0];
  baml.math.median(xs);
  assert.is_true(baml.deep_equals(xs, [3.0, 1.0, 2.0]))
}

test "math_median_empty_throws" {
  {
    baml.math.median([]);
    baml.sys.panic("expected baml.math.median([]) to throw")
  } catch (e) {
    baml.errors.InvalidArgument => null
  }
}
