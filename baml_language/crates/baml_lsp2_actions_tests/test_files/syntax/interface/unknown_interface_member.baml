// `bark()` is not declared on `Animal` — providing it in `implements Animal {}` is an error.

interface Animal {
  function speak(self) -> string
}

class Dog {
  implements Animal {
    function speak(self) -> string {
      return "Woof!"
    }
    function bark(self) -> string {
      return "Bark!"
    }
  }
}

//----
//- diagnostics
// error: method `bark` is not declared on interface `user.Animal`; remove it or add it to the interface
//     ╭─[ interface_unknown_interface_member.baml:12:14 ]
//     │
//  12 │     function bark(self) -> string {
//     │              ──┬─  
//     │                ╰─── not a member of the interface
//     │ 
//     │ Note: Error code: E0115
// ────╯
