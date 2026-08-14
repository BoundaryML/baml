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
  reflect.type_of<int>()
}

function reflected_container() -> type {
  reflect.type_of<map<string, int[]>>()
}

function reflected_union() -> type {
  reflect.type_of<int | string>()
}

function reflected_primitives() -> type[] {
  [
    reflect.type_of<string>(),
    reflect.type_of<int>(),
    reflect.type_of<bigint>(),
    reflect.type_of<float>(),
    reflect.type_of<bool>(),
    reflect.type_of<null>(),
    reflect.type_of<uint8array>(),
    reflect.type_of<type>(),
  ]
}

function reflected_literals() -> type[] {
  [
    reflect.type_of<"literal">(),
    reflect.type_of<42>(),
    reflect.type_of<42n>(),
    reflect.type_of<true>(),
  ]
}

function get_reflected_class() -> type {
  reflect.type_of<ReflectedClass>()
}

function get_reflected_enum() -> type {
  reflect.type_of<ReflectedEnum>()
}

function get_reflected_alias() -> type {
  reflect.type_of<ReflectedAlias>()
}

function reflected_generic_class() -> type {
  reflect.type_of<user.generic_tests.GenericBox<int>>()
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
