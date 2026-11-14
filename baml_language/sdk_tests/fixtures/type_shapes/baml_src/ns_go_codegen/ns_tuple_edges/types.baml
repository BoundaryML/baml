// Go-specific tuple calling conventions. Go has no tuple type: a generated
// function whose return type is a tuple returns one Go result per slot beside
// its error, and a failure returns each slot's zero value. A tuple whose slots
// mention a type parameter has no generated struct and crosses as `[]any`.

function pair_or_fail(fail: bool) -> (int, string) throws string {
  if (fail) {
    throw "boom"
  }
  (7, "seven")
}

// Expanded results keep each slot's own type, including a type parameter.
function tag_value<T>(value: T) -> (T, int) {
  (value, 1)
}

// Below the top level the tuple over `T` is the `[]any` fallback.
function tag_values<T>(value: T) -> (T, int)[] {
  [(value, 1), (value, 2)]
}

// Four arms exceed Go's typed-union threshold, so this union is `any`; the
// engine's selected arm still finds the generated tuple struct by identity.
function round_trip_dynamic_tuple(value: (int, string) | int | string | bool) -> (int, string) | int | string | bool {
  value
}

// A host callback that throws a tuple: Go restates a tuple it received by
// the tuple type the engine carried beside it, so a callback can throw on
// the BAML error it got from `throw_pair` and BAML still catches the tuple.
function catch_callback_pair(callback: () -> int throws (int, string)) -> int {
  callback() catch (e) {
    _ => e[0]
  }
}

function propagate_callback_pair(callback: () -> int throws (int, string)) -> int throws (int, string) {
  callback()
}

// A thrown tuple whose type no Go signature names, so no named struct
// registers it: the error's value is the list of its slot types' join.
function throw_unnamed_pair() -> int throws (bool, bool) {
  throw (true, false)
}
