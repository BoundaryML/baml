/// Returns a deep copy of `value` — all nested structures are recursively cloned.
//baml:mut_vm
function deep_copy<T>(value: T) -> T {
  $rust_function
}

/// Returns `true` if `a` and `b` are deeply equal — recursively comparing all nested values.
//baml:vm
function deep_equals<T>(a: T, b: T) -> bool {
  $rust_function
}

/// Returns the larger of two numeric values.
///
/// Both arguments must be the same numeric type: either both `int` or both
/// `float`. The return type is the same as the argument type.
///
/// # Examples
/// ```
/// baml.max(3, 5)       // 5
/// baml.max(3.0, 5.0)   // 5.0
/// ```
//baml:mut_vm
function max<T>(a: T, b: T) -> T {
  $rust_function
}

/// Returns the smaller of two numeric values.
///
/// Both arguments must be the same numeric type: either both `int` or both
/// `float`. The return type is the same as the argument type.
///
/// # Examples
/// ```
/// baml.min(3, 5)       // 3
/// baml.min(3.0, 5.0)   // 3.0
/// ```
//baml:mut_vm
function min<T>(a: T, b: T) -> T {
  $rust_function
}
