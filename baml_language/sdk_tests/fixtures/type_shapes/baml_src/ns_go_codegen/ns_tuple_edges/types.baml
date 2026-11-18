
function pair_or_fail(fail: bool) -> (int, string) throws string {
  if (fail) {
    throw "boom"
  }
  (7, "seven")
}

function tag_value<T>(value: T) -> (T, int) {
  (value, 1)
}

function tag_values<T>(value: T) -> (T, int)[] {
  [(value, 1), (value, 2)]
}

function round_trip_dynamic_tuple(value: (int, string) | int | string | bool) -> (int, string) | int | string | bool {
  value
}

function catch_callback_pair(callback: () -> int throws (int, string)) -> int {
  callback() catch (e) {
    _ => e.0
  }
}

function propagate_callback_pair(callback: () -> int throws (int, string)) -> int throws (int, string) {
  callback()
}

function throw_unnamed_pair() -> int throws (bool, bool) {
  throw (true, false)
}
