//baml:mut_vm
function new(key: reflect.Type, value: reflect.Type) -> Type throws never {
  $rust_function
}

/// A map-kind view of a `type` value. Users never construct this class.
class Type {
  /// The wrapped `type` value. Private by invariant: reflection natives are
  /// its only readers, and each kind method panics if the held type no longer
  /// classifies as this view's kind.
  _ty reflect.Type

  implements reflect.TypeView {
    //baml:vm
    //baml:fallible
    function as_type(self) -> reflect.Type throws never { $rust_function }
  }
  //baml:mut_vm
  //baml:fallible
  function key_type(self) -> reflect.Type throws never { $rust_function }
  //baml:mut_vm
  //baml:fallible
  function value_type(self) -> reflect.Type throws never { $rust_function }
}
