// BEP-044 Field Conflict Rule 5: an interface's field may be typed as the
// interface itself, and implementors carry that field as the interface type.

interface Animal {
  parent: Animal?

  function speak(self) -> string throws never
}

class Dog {
  parent: Animal?

  implements Animal {
    function speak(self) -> string { return "Woof!" }
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
