/// An interface-kind view of a `type` value. Users never construct this class.
class Type {
  implements baml.reflect.TypeView {
    //baml:vm
    function as_type(self) -> type throws never { $rust_function }
  }
  //baml:vm
  function implemented_by(self, other: type) -> bool throws never { $rust_function }
}

/// An immutable structural-conformance witness.  Values are created only by
/// `implementation<I>()`; each `field` call returns a new witness.
class Implementation {
  _handle $rust_type

  //baml:mut_vm
  function field(
    self,
    interface_field: string,
    class_field: string? = null,
  ) -> Implementation throws baml.reflect.errors.CompilationError {
    $rust_function
  }
}

/// Start a structural witness for a static interface.
//baml:mut_vm
function implementation<I>() -> Implementation throws baml.reflect.errors.CompilationError {
  $rust_function
}
