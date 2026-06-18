/// Returns a deep copy of `value` — all nested structures are recursively cloned.
//baml:mut_vm
function deep_copy<T>(value: T) -> T throws never {
  $rust_function
}

/// Returns `true` if `a` and `b` are deeply equal — recursively comparing all nested values.
//baml:vm
function deep_equals<T>(a: T, b: T) -> bool throws never {
  $rust_function
}

/// Implemented by all concrete types.
/// Can be used to check or bound a type to only include concrete types.
interface Concrete {

}

implement<T> Concrete for T {

}
