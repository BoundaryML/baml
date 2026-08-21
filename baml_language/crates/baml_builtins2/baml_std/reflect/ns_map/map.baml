//baml:mut_vm
function new(key: reflect.Type, value: reflect.Type) -> Type throws never {
  $rust_function
}

/// A map-kind view of a `type` value. Users never construct this class.
class Type {
  implements reflect.TypeView {
    //baml:vm
    function as_type(self) -> reflect.Type throws never { $rust_function }
  }
  //baml:mut_vm
  function key_type(self) -> reflect.Type throws never { $rust_function }
  //baml:mut_vm
  function value_type(self) -> reflect.Type throws never { $rust_function }
}
