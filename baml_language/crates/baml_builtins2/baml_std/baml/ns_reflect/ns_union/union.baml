//baml:mut_vm
function new(types: type[]) -> Type throws baml.reflect.errors.CompilationError {
  $rust_function
}

/// A union-kind view of a `type` value. Users never construct this class.
class Type {
  implements baml.reflect.TypeView {
    //baml:vm
    function as_type(self) -> type throws never { $rust_function }
  }
  //baml:mut_vm
  function member_types(self) -> type[] throws never { $rust_function }
}
