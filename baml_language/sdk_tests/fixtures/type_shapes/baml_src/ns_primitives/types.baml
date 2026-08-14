// Primitive Ty variants — one row per `Primitive(*)` from 18a §"Type shapes".
//
// `Primitives` exposes one field per variant so the type rendering ends up
// in the generated Python. The matching `Return*` functions cover the BAML
// frontend for the same variants but are not emitted into Python (see 18b
// §"Pure-expression functions don't emit Python") — we keep them around so
// the HIR/TIR sees these signatures.

class Primitives {
  int_field int
  float_field float
  string_field string
  bool_field bool
  null_field null
  uint8array_field uint8array
}

function return_int() -> int {
  42
}

function return_float() -> float {
  3.14
}

function return_string() -> string {
  "hello"
}

function return_bool() -> bool {
  true
}

// Keep bigint standalone for now so primitive function codegen is covered
// without changing the shared Primitives class fixtures in other SDKs.
function return_bigint() -> bigint {
  12345678901234567890n
}

function return_null() -> null {
  null
}

function round_trip_uint8_array(b: uint8array) -> uint8array {
  b
}

function round_trip_int(x: int) -> int {
  x
}

function round_trip_float(x: float) -> float {
  x
}

function round_trip_string(x: string) -> string {
  x
}

function round_trip_bool(x: bool) -> bool {
  x
}

function round_trip_bigint(x: bigint) -> bigint {
  x
}

function round_trip_null(x: null) -> null {
  x
}

function round_trip_primitives(p: Primitives) -> Primitives {
  p
}
