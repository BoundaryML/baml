//baml:mut_vm
function deep_copy<T>(value: T) -> T {
  $rust_function
}

//baml:vm
function deep_equals<T>(a: T, b: T) -> bool {
  $rust_function
}

/// Returns true if `a` and `b` are the same object, false otherwise.
/// This checks that they have the same object pointers, not value equality.
///
/// Will always return `false` for by-copy primitives (`null`, `int`, `float`, `bool`).
function ref_equals(a: unknown, b: unknown) -> bool {
    $rust_function
}
