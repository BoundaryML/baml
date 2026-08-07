class Field {
  name string
  type type?
  meta baml.reflect.Meta
}

/// Construct a closed runtime class. Map iteration order is definition order.
function new(
  name: string,
  fields: map<string, type | baml.reflect.WithMeta<type>>,
  implementations: baml.reflect.interface.Implementation[] = [],
) -> Type throws baml.reflect.errors.CompilationError {
  // Native functions receive the omitted-argument sentinel instead of
  // evaluating BAML default expressions. Keep the public API as the BEP
  // specifies and resolve the default in this BAML wrapper before crossing
  // the native boundary.
  _new(name, fields, implementations)
}

/// Internal native backing `new`; all arguments have been materialized.
//baml:mut_vm
function _new(
  name: string,
  fields: map<string, type | baml.reflect.WithMeta<type>>,
  implementations: baml.reflect.interface.Implementation[],
) -> Type throws baml.reflect.errors.CompilationError {
  $rust_function
}

/// Assert-read a named field from a runtime-shaped class instance.
//baml:mut_vm
function get_field<T>(value: unknown, name: string) -> T throws baml.reflect.errors.CompilationError {
  $rust_function
}

/// A class-kind view of a `type` value. Users never construct this class.
class Type {
  implements baml.reflect.TypeView {
    //baml:vm
    function as_type(self) -> type throws never { $rust_function }
  }
  //baml:mut_vm
  function fields(self) -> Field[] throws never { $rust_function }
  //baml:mut_vm
  function meta(self) -> baml.reflect.Meta throws never { $rust_function }
}
