// BEP-044: old `obj.Interface.method()` projection syntax is rejected. Use
// `obj.as<Interface>.method()` instead.

interface Animal {
  function speak(self) -> string throws never
}

class Dog {
  implements Animal {
    function speak(self) -> string { return "Woof!" }
  }
}

function bad() -> string {
  let d = Dog {}
  return d.Animal.speak()
}

//----
//- diagnostics
// E0007
//
//   × type `Dog` has no member `Animal`
//     ╭─[interface_error_old_qualified_projection.baml:16:10]
//  16 │   return d.Animal.speak()
//     ·          ──────────────
//     ╰────
