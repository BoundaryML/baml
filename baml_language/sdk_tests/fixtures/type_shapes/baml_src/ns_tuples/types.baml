
class TupleHolder {
  pair (int, string)
  nested ((int, string), bool)
  maybe (int, string?)?
  pairs (string, int)[]
}

class TupleBox<T> {
  value T
}

function round_trip_pair(t: (int, string)) -> (int, string) {
  t
}

function round_trip_single(t: (string,)) -> (string,) {
  t
}

function round_trip_repeated(t: (int, int, int)) -> (int, int, int) {
  t
}

function round_trip_nested(t: ((int, string), bool)) -> ((int, string), bool) {
  t
}

function round_trip_optional_slot(t: (int, string?)) -> (int, string?) {
  t
}

function round_trip_optional_tuple(t: (int, string)?) -> (int, string)? {
  t
}

function round_trip_tuple_list(xs: (string, int)[]) -> (string, int)[] {
  xs
}

function round_trip_tuple_map(m: map<string, (int, bool)>) -> map<string, (int, bool)> {
  m
}

function round_trip_tuple_or_string(u: (int, string) | string) -> (int, string) | string {
  u
}

function round_trip_tuple_arms(u: (int, string) | (string, int)) -> (int, string) | (string, int) {
  u
}

function tuple_or_list_is_tuple(u: (int, int) | int[]) -> bool {
  reflect.Type.of_value(u) == reflect.Type.of<(int, int)>()
}

function round_trip_holder(h: TupleHolder) -> TupleHolder {
  h
}

function round_trip_tuple_box(b: TupleBox<(int, string)>) -> TupleBox<(int, string)> {
  b
}

function make_pair(n: int, s: string) -> (int, string) {
  (n, s)
}

function swap_pair(t: (int, string)) -> (string, int) {
  (t.1, t.0)
}

function pair_first(t: (int, string)) -> int {
  t.0
}

function call_tuple_callback(
  callback: (t: (int, string)) -> (string, int),
  t: (int, string),
) -> (string, int) {
  callback(t)
}

function throw_pair(t: (int, string)) -> int throws (int, string) {
  throw t
}

function round_trip_wide(
  t: (int, string, int, string, int, string, int, string, int, string, int, string, int),
) -> (int, string, int, string, int, string, int, string, int, string, int, string, int) {
  t
}

function tuple_as_unknown(t: (int | string, bool)) -> unknown {
  t
}

function unknown_has_carried_tuple_type(v: unknown) -> bool {
  reflect.Type.of_value(v) == reflect.Type.of<(int | string, bool)>()
}

function value_has_type<T>(v: unknown) -> bool {
  reflect.Type.of_value(v) == reflect.Type.of<T>()
}

function call_unknown_tuple_callback(
  callback: (v: unknown) -> unknown,
  t: (int | string, bool),
) -> bool {
  unknown_has_carried_tuple_type(callback(t))
}

function tuple_generic_identity<T>(x: T) -> T {
  x
}
