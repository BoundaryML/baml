/// An interface-kind view of a `type` value. Users never construct this class.
class Type {
  implements baml.reflect.TypeView {
    //baml:vm
    function as_type(self) -> type throws never { $rust_function }
  }
  //baml:vm
  function implemented_by(self, other: type) -> bool throws never { $rust_function }
  //baml:mut_vm
  function implementors(self) -> type[] throws never { $rust_function }
}
