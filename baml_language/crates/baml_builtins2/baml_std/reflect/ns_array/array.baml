/// An array-kind view of a `type` value. Users never construct this class.
class Type {
  implements reflect.TypeView {
    //baml:vm
    function as_type(self) -> reflect.Type throws never { $rust_function }
  }
  //baml:mut_vm
  function element_type(self) -> reflect.Type throws never { $rust_function }
}
