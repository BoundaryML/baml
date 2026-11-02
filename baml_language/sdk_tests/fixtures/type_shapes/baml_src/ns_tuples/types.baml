// Tuple rows: fixed-arity products crossing the host boundary in every
// position a generated SDK reaches -- arguments, returns, class fields,
// list / map / optional elements, union arms, nested tuples, generic
// arguments, host callbacks, and thrown values. Slot order and repeated
// slot types are part of the identity: `(int, string)` and `(string, int)`
// are distinct, and `(int, int, int)` keeps three slots.

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

// Two same-arity tuple arms: the engine's selected arm picks the case.
function round_trip_tuple_arms(u: (int, string) | (string, int)) -> (int, string) | (string, int) {
  u
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
  (t[1], t[0])
}

function pair_first(t: (int, string)) -> int {
  t[0]
}

// A host callback receiving and returning tuples.
function call_tuple_callback(
  callback: (t: (int, string)) -> (string, int),
  t: (int, string),
) -> (string, int) {
  callback(t)
}

function throw_pair(t: (int, string)) -> int throws (int, string) {
  throw t
}

// Thirteen slots: wider than some hosts' native or runtime tuple families,
// so those hosts take their wide-tuple representation.
function round_trip_wide(
  t: (int, string, int, string, int, string, int, string, int, string, int, string, int),
) -> (int, string, int, string, int, string, int, string, int, string, int, string, int) {
  t
}
