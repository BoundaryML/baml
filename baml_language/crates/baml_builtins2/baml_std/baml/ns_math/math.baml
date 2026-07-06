/// Truncates a float toward zero and returns the integer part.
function trunc(value: float) -> int throws never {
  $rust_function
}

/// Returns the sum of every element in `values`.
///
/// The elements are added left-to-right starting from `0.0`, so the empty
/// array sums to `0.0`; the result is always a `float`. Never throws.
///
/// `int` and `float` are distinct types with no implicit widening, so an
/// `int[]` is not accepted here — map integer data to floats first, e.g.
/// `ints.map((x: int) -> float { x * 1.0 })`.
///
/// # Examples
/// ```
/// baml.math.sum([1.0, 2.0, 3.0])   // 6.0
/// baml.math.sum([-1.0, 2.0])       // 1.0
/// baml.math.sum([])                // 0.0
/// ```
//baml:vm
function sum(values: float[]) -> float throws never {
  $rust_function
}

/// Sums an `int[]` left-to-right starting from `0`, returning an `int`.
///
/// Internal helper backing `int[].sum()` (the `Summable` method); prefer the
/// method form `ints.sum()` at call sites. Like repeated `+`, every running
/// total must stay within the `int` range, so a sum that leaves it raises
/// `baml.panics.IntegerOverflow`. The empty array sums to `0`. Internal (the
/// leading underscore is the convention for non-public stdlib helpers).
//baml:fallible
function _sum_int(values: int[]) -> int throws never {
  $rust_function
}

/// Returns the arithmetic mean (average) of `values`.
///
/// Computed as `sum(values) / values.length()`. Throws
/// `root.errors.InvalidArgument` when `values` is empty, because the mean of
/// no elements is undefined.
///
/// # Examples
/// ```
/// baml.math.mean([1.0, 2.0, 3.0, 4.0])   // 2.5
/// baml.math.mean([5.0])                  // 5.0
/// baml.math.mean([])                     // throws InvalidArgument
/// ```
//baml:vm
function mean(values: float[]) -> float throws root.errors.InvalidArgument {
  $rust_function
}

/// Returns the median (the middle value) of `values`.
///
/// The elements are copied and sorted ascending before the middle is taken, so
/// the caller's array is left untouched. With an odd element count the single
/// middle element is returned; with an even count the arithmetic mean of the
/// two middle elements is returned. Float ordering follows IEEE 754
/// `totalOrder`, matching `float[].sort()`. Throws
/// `root.errors.InvalidArgument` when `values` is empty, because the median of
/// no elements is undefined.
///
/// # Examples
/// ```
/// baml.math.median([3.0, 1.0, 2.0])        // 2.0  (odd count)
/// baml.math.median([1.0, 2.0, 3.0, 4.0])   // 2.5  (mean of the two middles)
/// baml.math.median([])                     // throws InvalidArgument
/// ```
//baml:vm
function median(values: float[]) -> float throws root.errors.InvalidArgument {
  $rust_function
}
