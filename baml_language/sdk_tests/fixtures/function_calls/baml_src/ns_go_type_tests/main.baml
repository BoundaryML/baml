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
  int.type()
}

function reflected_container() -> reflect.Type {
  reflect.Type.of<map<string, int[]>>()
}

function reflected_union() -> reflect.Type {
  reflect.Type.of<int | string>()
}

function reflected_primitives() -> reflect.Type[] {
  [
    string.type(),
    int.type(),
    bigint.type(),
    float.type(),
    bool.type(),
    reflect.Type.of<null>(),
    uint8array.type(),
    reflect.Type.type(),
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
  ReflectedClass.type()
}

function get_reflected_enum() -> reflect.Type {
  ReflectedEnum.type()
}

function get_reflected_alias() -> reflect.Type {
  reflect.Type.of<ReflectedAlias>()
}

function reflected_generic_class() -> reflect.Type {
  root.generic_tests.GenericBox<int>.type()
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
