class TypeBox {
  direct type
  optional type?
  list type[]
  mapping map<string, type>
}

class ReflectedClass {
  value string
}

enum ReflectedEnum {
  FIRST
  SECOND
}

type ReflectedAlias = string[]

function type_name(value: type) -> string {
  value.to_string()
}

function round_trip_type(value: type) -> type {
  value
}

function reflected_int() -> type {
  type.of<int>()
}

function reflected_container() -> type {
  type.of<map<string, int[]>>()
}

function reflected_union() -> type {
  type.of<int | string>()
}

function reflected_primitives() -> type[] {
  [
    type.of<string>(),
    type.of<int>(),
    type.of<bigint>(),
    type.of<float>(),
    type.of<bool>(),
    type.of<null>(),
    type.of<uint8array>(),
    type.of<type>(),
  ]
}

function reflected_literals() -> type[] {
  [
    type.of<"literal">(),
    type.of<42>(),
    type.of<42n>(),
    type.of<true>(),
  ]
}

function get_reflected_class() -> type {
  type.of<ReflectedClass>()
}

function get_reflected_enum() -> type {
  type.of<ReflectedEnum>()
}

function get_reflected_alias() -> type {
  type.of<ReflectedAlias>()
}

function reflected_generic_class() -> type {
  type.of<user.generic_tests.GenericBox<int>>()
}

function round_trip_optional_type(value: type?) -> type? {
  value
}

function default_optional_type(value: type? = null) -> type? {
  value
}

function round_trip_type_list(value: type[]) -> type[] {
  value
}

function round_trip_type_map(value: map<string, type>) -> map<string, type> {
  value
}

function round_trip_type_box(value: TypeBox) -> TypeBox {
  value
}

function round_trip_type_or_string(value: type | string) -> type | string {
  value
}

function round_trip_dynamic_type_union(value: type | string | int | bool) -> type | string | int | bool {
  value
}

function call_type_callback(callback: (type) -> type, value: type) -> type {
  callback(value)
}
