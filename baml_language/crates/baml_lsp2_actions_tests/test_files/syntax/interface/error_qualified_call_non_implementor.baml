// BEP-044: `.as<I>` projection to an interface the receiver doesn't implement —
// `d.as<Swimmer>.swim()` when `Dog` only implements `Animal`.

interface Animal {
  function speak(self) -> string
}

interface Swimmer {
  function swim(self) -> string
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
// error: type mismatch: expected user.Swimmer, got user.Dog
//     ╭─[ interface_error_qualified_call_non_implementor.baml:19:10 ]
//     │
//  19 │   return d.as<Swimmer>.swim()
//     │          ──────┬──────  
//     │                ╰──────── type mismatch: expected user.Swimmer, got user.Dog
//     │ 
//     │ Note: Error code: E0001
// ────╯
