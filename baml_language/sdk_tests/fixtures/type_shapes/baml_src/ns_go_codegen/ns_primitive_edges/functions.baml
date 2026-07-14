// Literal bigint and float types exercise the two primitive-literal branches
// that are not represented by class-field fixtures in the shared corpus.
function round_trip_literal_bigint(value: 42n) -> 42n {
  value
}

// These identifiers are legal BAML names but collide with Go import aliases
// or predeclared types used by the generated function body.
function reserved_type_names(
  big: bigint,
  int64: int,
  float64: float,
  byte: uint8array,
  context: string,
  error: string,
) -> bigint {
  big
}

class PrimitiveHolder {
  big_value bigint
  optional_int int?
}

// Exhaustive first-stage class wire fixture. Keep optional and nested fields
// out of this class so every supported primitive is exercised independently.
class WirePrimitives {
  string_value string
  int_value int
  bigint_value bigint
  float_value float
  bool_value bool
  null_value null
  bytes_value uint8array
}

function round_trip_wire_primitives(value: WirePrimitives) -> WirePrimitives {
  value
}
