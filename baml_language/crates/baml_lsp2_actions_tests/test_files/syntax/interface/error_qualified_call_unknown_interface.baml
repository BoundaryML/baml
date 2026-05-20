// BEP-044 §"Method Disambiguation": `obj.Foo.method()` where `Foo` is not an
// interface name visible to the class — diagnostic should explain that the
// qualifier must be a known interface.

interface Animal {
  function speak(self) -> string
}

class Dog {
  implements Animal {
    function speak(self) -> string { return "Woof!" }
  }
}

function bad(d: Dog) -> string {
  return d.NotAnInterface.speak()
}

//----
//- diagnostics
// Error: type `user.Dog` has no member `NotAnInterface`
//     ╭─[ interface_error_qualified_call_unknown_interface.baml:16:12 ]
//     │
//  16 │   return d.NotAnInterface.speak()
//     │            ───────┬──────  
//     │                   ╰──────── type `user.Dog` has no member `NotAnInterface`
//     │ 
//     │ Note: Error code: E0007
// ────╯
