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
