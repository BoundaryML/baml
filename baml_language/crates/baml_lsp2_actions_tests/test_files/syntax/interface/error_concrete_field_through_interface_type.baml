// BEP-044: accessing a concrete-class-only field through an interface-typed
// variable is a compile error — only members of the interface are visible.

interface Animal {
  name: string
  function speak(self) -> string throws never
}

class Dog {
  name: string
  breed: string

  implements Animal {
    function speak(self) -> string { return "Woof!" }
  }
}

function bad(a: Animal) -> string {
  return a.breed
}

//----
//- diagnostics
// E0007
//
//   × type `Animal` has no member `breed`
//     ╭─[interface_error_concrete_field_through_interface_type.baml:19:10]
//  19 │   return a.breed
//     ·          ───────
//     ╰────
