class TypeBox {
  direct reflect.Type
  optional reflect.Type?
  list reflect.Type[]
  mapping map<string, reflect.Type>
}

class ReflectedClass {
  value string
}

enum ReflectedEnum {
  FIRST
  SECOND
}

type ReflectedAlias = string[]

function type_name(value: reflect.Type) -> string {
  value.to_string()
}

function round_trip_type(value: reflect.Type) -> reflect.Type {
  value
}

function reflected_int() -> reflect.Type {
  reflect.Type.of<int>()
}

function reflected_container() -> reflect.Type {
  reflect.Type.of<map<string, int[]>>()
}

function reflected_union() -> reflect.Type {
  reflect.Type.of<int | string>()
}

function reflected_primitives() -> reflect.Type[] {
  [
    reflect.Type.of<string>(),
    reflect.Type.of<int>(),
    reflect.Type.of<bigint>(),
    reflect.Type.of<float>(),
    reflect.Type.of<bool>(),
    reflect.Type.of<null>(),
    reflect.Type.of<uint8array>(),
    reflect.Type.of<reflect.Type>(),
  ]
}

function reflected_literals() -> reflect.Type[] {
  [
    reflect.Type.of<"literal">(),
    reflect.Type.of<42>(),
    reflect.Type.of<42n>(),
    reflect.Type.of<true>(),
  ]
}

function get_reflected_class() -> reflect.Type {
  reflect.Type.of<ReflectedClass>()
}

function get_reflected_enum() -> reflect.Type {
  reflect.Type.of<ReflectedEnum>()
}

function get_reflected_alias() -> reflect.Type {
  reflect.Type.of<ReflectedAlias>()
}

function reflected_generic_class() -> reflect.Type {
  reflect.Type.of<user.generic_tests.GenericBox<int>>()
}

function round_trip_optional_type(value: reflect.Type?) -> reflect.Type? {
  value
}

function default_optional_type(value: reflect.Type? = null) -> reflect.Type? {
  value
}

function round_trip_type_list(value: reflect.Type[]) -> reflect.Type[] {
  value
}

function round_trip_type_map(value: map<string, reflect.Type>) -> map<string, reflect.Type> {
  value
}

function round_trip_type_box(value: TypeBox) -> TypeBox {
  value
}

function round_trip_type_or_string(value: reflect.Type | string) -> reflect.Type | string {
  value
}

function round_trip_dynamic_type_union(value: reflect.Type | string | int | bool) -> reflect.Type | string | int | bool {
  value
}

function call_type_callback(callback: (reflect.Type) -> reflect.Type, value: reflect.Type) -> reflect.Type {
  callback(value)
}
