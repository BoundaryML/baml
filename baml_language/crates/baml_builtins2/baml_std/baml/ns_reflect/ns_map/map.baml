//baml:mut_vm
function new(key: type, value: type) -> Type throws never {
  $rust_function
}

/// A map-kind view of a `type` value. Users never construct this class.
class Type {
  implements baml.reflect.TypeView {
    //baml:vm
    function as_type(self) -> type throws never { $rust_function }
  }
  //baml:mut_vm
  function key_type(self) -> type throws never { $rust_function }
  //baml:mut_vm
  function value_type(self) -> type throws never { $rust_function }
}
