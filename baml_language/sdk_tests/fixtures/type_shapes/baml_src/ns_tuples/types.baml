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

// A tuple held as `unknown` keeps its carried type: `(1, true)` here is an
// `(int | string, bool)`, never the `(int, bool)` its elements suggest. A
// host that receives it with no declared tuple type and sends it back must
// restate that same tuple.
function tuple_as_unknown(t: (int | string, bool)) -> unknown {
  t
}

function unknown_has_carried_tuple_type(v: unknown) -> bool {
  reflect.Type.of_value(v) == reflect.Type.of<(int | string, bool)>()
}

// Whether `v`'s own (carried) type is exactly `T`: a host binds `T` to a
// tuple type with its reflected type token.
function value_has_type<T>(v: unknown) -> bool {
  reflect.Type.of_value(v) == reflect.Type.of<T>()
}

// A host callback receiving a tuple at an open slot and returning it there.
function call_unknown_tuple_callback(
  callback: (v: unknown) -> unknown,
  t: (int | string, bool),
) -> bool {
  unknown_has_carried_tuple_type(callback(t))
}

// A value at a type parameter the call leaves open: the host states its type,
// so a host tuple binds `T` to a tuple type and comes back a tuple.
function tuple_generic_identity<T>(x: T) -> T {
  x
}
