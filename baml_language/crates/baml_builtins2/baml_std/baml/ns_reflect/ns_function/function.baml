class Parameter {
  name string?
  type type
  optional bool
}

/// A function-kind view of a `type` value. Throws are intentionally not part
/// of the function reflection surface.
class Type {
  implements baml.reflect.TypeView {
    //baml:vm
    function as_type(self) -> type throws never { $rust_function }
  }
  //baml:mut_vm
  function params(self) -> Parameter[] throws never { $rust_function }
  //baml:mut_vm
  function return_type(self) -> type throws never { $rust_function }
}
