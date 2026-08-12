// BEP-044: a method whose only `Self` parameter is the receiver may be called
// through an interface-typed value; `Self` in the return type collapses to the
// interface type.

interface Cloneable {
  value: int
  function clone(self) -> Self throws never
}

class Box {
  value: int

  implements Cloneable {
    function clone(self) -> Self {
      return Box { value: self.value }
    }
  }
}

function read_clone(c: Cloneable) -> int {
  return c.clone().value
}

//----
//- diagnostics
// <no-diagnostics-expected>
