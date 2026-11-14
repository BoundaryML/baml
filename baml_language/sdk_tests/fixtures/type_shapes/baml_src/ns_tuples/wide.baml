// Tuples wider than some hosts' native tuple support (Rust's standard
// library implements `Debug` and `PartialEq` for tuples of at most 12
// slots, serde for at most 16) in more positions: twenty slots, a class
// field, a union arm, and a slot holding a type parameter.

class WideTupleHolder {
  wide (int, string, int, string, int, string, int, string, int, string, int, string, int)
  label string
}

function round_trip_wide20(
  t: (int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int),
) -> (int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int, int) {
  t
}

function round_trip_wide_holder(h: WideTupleHolder) -> WideTupleHolder {
  h
}

function round_trip_wide_or_string(
  u: (int, string, int, string, int, string, int, string, int, string, int, string, int) | string,
) -> (int, string, int, string, int, string, int, string, int, string, int, string, int) | string {
  u
}

// A tuple whose slot is a type parameter the call binds.
function pair_with<T>(x: T, n: int) -> (T, int) {
  (x, n)
}
