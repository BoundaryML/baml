
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

function pair_with<T>(x: T, n: int) -> (T, int) {
  (x, n)
}
