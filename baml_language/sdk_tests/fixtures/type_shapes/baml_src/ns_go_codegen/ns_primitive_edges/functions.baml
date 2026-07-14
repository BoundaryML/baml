// Literal bigint exercises the primitive-literal branch not represented by
// class-field fixtures in the shared corpus. BAML currently rejects float
// literal types at parse time.
function round_trip_literal_bigint(value: 42n) -> 42n {
  value
}

// Generator-owned locals end in `_`, and package aliases consult the same
// protected-name table. These ordinary BAML parameters must therefore remain
// usable without shadowing any emitted identifier; `nil` is escaped because
// it is a Go predeclared identifier.
function protected_local_names(
  nil: string,
  ctx: string,
  err: string,
  result: string,
  zero: string,
  options: string,
  arguments: string,
  value: string,
  decoded: string,
  bootstrap: string,
) -> string {
  nil
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

class NullableWire {
  string_value string?
  int_value int?
  bigint_value bigint?
  float_value float?
  bool_value bool?
  bytes_value uint8array?
  class_value WirePrimitives?
}

function round_trip_nullable_wire(value: NullableWire) -> NullableWire {
  value
}

// Spell both union orders explicitly so codegen does not depend on `?` sugar
// or on null appearing in one particular position.
function round_trip_optional_string(value: null | string) -> string | null {
  value
}

function round_trip_optional_int(value: int?) -> int? {
  value
}

function round_trip_optional_bigint(value: bigint?) -> bigint? {
  value
}

function round_trip_optional_float(value: float?) -> float? {
  value
}

function round_trip_optional_bool(value: bool?) -> bool? {
  value
}

function round_trip_optional_bytes(value: uint8array?) -> uint8array? {
  value
}

function round_trip_optional_class(value: WirePrimitives?) -> WirePrimitives? {
  value
}

// Container nullability matrix. Outer nullability and element/value
// nullability are independent and must never collapse into accidental **T.
class NullableContainers {
  optional_list string[]?
  optional_map map<string, int>?
  optional_elements string?[]
  optional_values map<string, int?>
  nested_lists int[][]
  map_of_optional_lists map<string, string?[]>
}

function round_trip_nullable_containers(value: NullableContainers) -> NullableContainers {
  value
}

function round_trip_optional_list(value: null | int[]) -> int[] | null {
  value
}

function round_trip_optional_map(value: null | map<string, int>) -> map<string, int> | null {
  value
}

function round_trip_optional_list_of_optional(value: string?[]?) -> string?[]? {
  value
}

// The compiler-to-codegen boundary must flatten and deduplicate this to the
// same semantic type as string?.
function round_trip_repeated_null(value: null | string | null) -> null | string | null {
  value
}

// Required containers are finite recursion boundaries: an empty slice ends
// the graph even though each element is a required class value.
class ContainerTree {
  value int
  children ContainerTree[]
}

function round_trip_container_tree(value: ContainerTree) -> ContainerTree {
  value
}

// Cross-product of every currently supported leaf through both built-in
// containers. Nullable members are separate from nullable containers.
class ContainerLeafMatrix {
  list_strings string[]
  list_ints int[]
  list_bigints bigint[]
  list_floats float[]
  list_bools bool[]
  list_nulls null[]
  list_bytes uint8array[]
  list_classes WirePrimitives[]
  list_optional_strings string?[]
  list_optional_ints int?[]
  list_optional_bigints bigint?[]
  list_optional_floats float?[]
  list_optional_bools bool?[]
  list_optional_bytes uint8array?[]
  list_optional_classes WirePrimitives?[]

  map_strings map<string, string>
  map_ints map<string, int>
  map_bigints map<string, bigint>
  map_floats map<string, float>
  map_bools map<string, bool>
  map_nulls map<string, null>
  map_bytes map<string, uint8array>
  map_classes map<string, WirePrimitives>
  map_optional_strings map<string, string?>
  map_optional_ints map<string, int?>
  map_optional_bigints map<string, bigint?>
  map_optional_floats map<string, float?>
  map_optional_bools map<string, bool?>
  map_optional_bytes map<string, uint8array?>
  map_optional_classes map<string, WirePrimitives?>
}

function round_trip_container_leaf_matrix(value: ContainerLeafMatrix) -> ContainerLeafMatrix {
  value
}
