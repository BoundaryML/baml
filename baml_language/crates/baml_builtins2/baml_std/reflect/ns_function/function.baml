class Parameter {
  name string?
  type reflect.Type
  optional bool
}

/// A function-kind view of a `type` value. Throws are intentionally not part
/// of the function reflection surface.
class Type {
  implements reflect.TypeView {
    //baml:vm
    function as_type(self) -> reflect.Type throws never { $rust_function }
  }
  //baml:mut_vm
  function params(self) -> Parameter[] throws never { $rust_function }
  //baml:mut_vm
  function return_type(self) -> reflect.Type throws never { $rust_function }
}
