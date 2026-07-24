// BEP-044: `.as<I>` projection to an interface the receiver doesn't implement —
// `d.as<Swimmer>.swim()` when `Dog` only implements `Animal`.

interface Animal {
  function speak(self) -> string throws never
}

interface Swimmer {
  function swim(self) -> string throws never
}

class Dog {
  implements Animal {
    function speak(self) -> string { return "Woof!" }
  }
}

function dunk(d: Dog) -> string {
  return d.as<Swimmer>.swim()
}

//----
//- diagnostics
// E0001
//
//   × type `Dog` does not implement interface `Swimmer`
//     ╭─[interface_error_qualified_call_non_implementor.baml:19:10]
//  19 │   return d.as<Swimmer>.swim()
//     ·          ─────────────
//     ╰────
