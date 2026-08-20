//baml:mut_vm
function new(types: reflect.Type[]) -> Type throws reflect.errors.CompilationError {
  $rust_function
}

/// A union-kind view of a `type` value. Users never construct this class.
class Type {
  implements reflect.TypeView {
    //baml:vm
    function as_type(self) -> reflect.Type throws never { $rust_function }
  }
  //baml:mut_vm
  function member_types(self) -> reflect.Type[] throws never { $rust_function }
}
