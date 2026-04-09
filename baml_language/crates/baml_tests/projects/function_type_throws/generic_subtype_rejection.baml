// === Generic subtype rejection ===
// Mismatched instantiations of the same class must be rejected.

class Box<T> {
  value: T
}

function take_string_box(b: Box<string>) -> null {
  null
}

// Should produce a type error: Box<int> is not assignable to Box<string>
function bad_box_mismatch() -> null {
  take_string_box(Box<int> { value: 1 })
}
