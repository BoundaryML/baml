// BEP-044: qualified call to an interface the receiver doesn't implement —
// `d.Swimmer.swim()` when `Dog` only implements `Animal`.

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
  return d.Swimmer.swim()
}

//----
//- diagnostics
// Error: type `user.Dog` has no member `Swimmer`
//     ╭─[ interface_error_qualified_call_non_implementor.baml:19:12 ]
//     │
//  19 │   return d.Swimmer.swim()
//     │            ───┬───  
//     │               ╰───── type `user.Dog` has no member `Swimmer`
//     │ 
//     │ Note: Error code: E0007
// ────╯
