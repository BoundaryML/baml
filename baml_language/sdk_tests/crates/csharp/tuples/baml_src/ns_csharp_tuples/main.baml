// Tuples across the C# boundary. Tuples whose C# tuple keeps their identity
// project to native `ValueTuple`s; wider tuples and tuples with literal or
// nullable-reference slots project to generated `TupleN_…` records.

class TupleHolder {
  pair (int, string)
  pairs (string, int)[]
  maybe (int, bool)?
  wide (int, int, int, int, int, int, int, int)

  function first(self) -> (int, string) {
    self.pair
  }
}

function echo_pair(value: (int, string)) -> (int, string) {
  value
}

function swap_pair(value: (int, string)) -> (string, int) {
  (value[1], value[0])
}

function echo_single(value: (int,)) -> (int,) {
  value
}

function echo_nested(value: ((int, string), bool)) -> ((int, string), bool) {
  value
}

function echo_wide(
  value: (int, int, int, int, int, int, int, int),
) -> (int, int, int, int, int, int, int, int) {
  value
}

function echo_literal(value: ("tag", int)) -> ("tag", int) {
  value
}

function echo_optional_slot(value: (string?, int)) -> (string?, int) {
  value
}

function echo_holder(value: TupleHolder) -> TupleHolder {
  value
}

function echo_union_of_tuples(
  value: (int, bool) | (string, bool),
) -> (int, bool) | (string, bool) {
  value
}

function echo_product_of_unions(value: (int | string, bool)) -> (int | string, bool) {
  value
}

function echo_tuple_map(value: map<string, (int, string)>) -> map<string, (int, string)> {
  value
}

function call_pair_callback(
  callback: ((int, string)) -> (string, int),
  value: (int, string),
) -> (string, int) {
  callback(value)
}

function make_unknown_tuple() -> unknown {
  (1, "one")
}

function echo_unknown(value: unknown) -> unknown {
  value
}

function throw_pair(value: (int, string)) -> int throws (int, string) {
  throw value
}
