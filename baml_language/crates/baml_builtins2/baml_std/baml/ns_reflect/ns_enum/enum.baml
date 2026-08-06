class Value {
  name string
  meta baml.reflect.Meta
}

/// An enum-kind view of a `type` value. Users never construct this class.
class Type {
  implements baml.reflect.TypeView {
    //baml:vm
    function as_type(self) -> type throws never { $rust_function }
  }
  //baml:mut_vm
  function values(self) -> Value[] throws never { $rust_function }
  //baml:mut_vm
  function meta(self) -> baml.reflect.Meta throws never { $rust_function }
}
