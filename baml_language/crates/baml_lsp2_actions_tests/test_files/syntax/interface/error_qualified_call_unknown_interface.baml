// BEP-044 §"Method Disambiguation": `obj.as<Foo>.method()` where `Foo` is not an
// interface name visible to the class — diagnostic should explain that the
// qualifier must be a known interface.

interface Animal {
  function speak(self) -> string throws never
}

class Dog {
  implements Animal {
    function speak(self) -> string { return "Woof!" }
  }
}

function bad(d: Dog) -> string {
  return d.as<NotAnInterface>.speak()
}

//----
//- diagnostics
// E0002
//
//   × unresolved type: NotAnInterface
//     ╭─[interface_error_qualified_call_unknown_interface.baml:16:15]
//  16 │   return d.as<NotAnInterface>.speak()
//     ·               ──────────────
//     ╰────
