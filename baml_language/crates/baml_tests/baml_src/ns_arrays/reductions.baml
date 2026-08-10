// Tests for the `float[]` statistical reductions `mean` / `median` — the
// `FloatStats` interface in stdlib `containers.baml`.
//
// B-712 removed the old `baml.math.{mean,median}` free functions; the reductions
// are now methods (`xs.mean()`, `xs.median()`) backed by the private natives
// `_mean_float` / `_median_float`. Exact (integer-valued / terminating-binary)
// float results are compared with `assert.equal`; empty inputs assert the
// `InvalidArgument` throw. (`sum` is covered separately in `sum.baml`.)

// ─── mean ─────────────────────────────────────────────────────────────────────

test "float_mean_basic" {
  assert.equal([1.0, 2.0, 3.0, 4.0].mean(), 2.5)
}

test "float_mean_single" {
  assert.equal([5.0].mean(), 5.0)
}

// The mean of no elements is undefined → InvalidArgument.
test "float_mean_empty_throws" {
  let xs: float[] = [];
  {
    xs.mean();
    baml.sys.panic("expected [].mean() to throw")
  } catch (e) {
    baml.errors.InvalidArgument => null
  }
}

// ─── median ───────────────────────────────────────────────────────────────────

// Odd count: the single middle element after sorting.
test "float_median_odd" {
  assert.equal([3.0, 1.0, 2.0].median(), 2.0)
}

// Even count: the arithmetic mean of the two middle elements after sorting.
test "float_median_even" {
  assert.equal([1.0, 2.0, 3.0, 4.0].median(), 2.5)
}

test "float_median_unsorted_input" {
  assert.equal([5.0, 1.0, 4.0, 2.0, 3.0].median(), 3.0)
}

// `median` sorts a copy, so the caller's array is observably untouched.
test "float_median_does_not_mutate_input" {
  let xs = [3.0, 1.0, 2.0];
  xs.median();
  assert.equal(xs, [3.0, 1.0, 2.0])
}

test "float_median_empty_throws" {
  let xs: float[] = [];
  {
    xs.median();
    baml.sys.panic("expected [].median() to throw")
  } catch (e) {
    baml.errors.InvalidArgument => null
  }
}
