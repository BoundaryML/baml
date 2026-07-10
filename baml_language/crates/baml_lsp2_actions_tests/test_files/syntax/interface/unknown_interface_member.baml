// `bark()` is not declared on `Animal` — providing it in `implements Animal {}` is an error.

interface Animal {
  function speak(self) -> string throws never
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
// error: `bark` is not a method of interface `Animal`
//     ╭─[ interface_unknown_interface_member.baml:12:5 ]
//     │
//  12 │ ╭─▶     function bark(self) -> string {
//     ┆ ┆
//  14 │ ├─▶     }
//     │ │
//     │ ╰─────────── `bark` is not a method of interface `Animal`
//     │
//     │     Note: Error code: E0115
// ────╯
