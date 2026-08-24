//baml:mut_vm
function new(value: string | int | bool) -> Type throws never {
  $rust_function
}

/// A literal-kind view of a `type` value. Users never construct this class.
class Type {
  implements reflect.TypeView {
    //baml:vm
    function as_type(self) -> reflect.Type throws never { $rust_function }
  }
}
