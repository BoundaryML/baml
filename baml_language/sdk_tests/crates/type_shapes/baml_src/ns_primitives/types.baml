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

function ReturnInt() -> int {
  42
}

function ReturnFloat() -> float {
  3.14
}

function ReturnString() -> string {
  "hello"
}

function ReturnBool() -> bool {
  true
}

function ReturnNull() -> null {
  null
}

function RoundTripUint8Array(b: uint8array) -> uint8array {
  b
}

function RoundTripInt(x: int) -> int {
  x
}

function RoundTripFloat(x: float) -> float {
  x
}

function RoundTripString(x: string) -> string {
  x
}

function RoundTripBool(x: bool) -> bool {
  x
}

function RoundTripNull(x: null) -> null {
  x
}

function RoundTripPrimitives(p: Primitives) -> Primitives {
  p
}
