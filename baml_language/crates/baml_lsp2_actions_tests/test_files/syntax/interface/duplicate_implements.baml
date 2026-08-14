// Two `implements Animal` blocks on the same class — only one is allowed.

interface Animal {
  function speak(self) -> string throws never
}

class Dog {
  implements Animal {
    function speak(self) -> string {
      return "Woof!"
    }
  }

  implements Animal {
    function speak(self) -> string {
      return "Woof again?"
    }
  }
}

//----
//- diagnostics
// E0132
//
//   × overlapping interface implementations for the same receiver/interface
//    ╭─[interface_duplicate_implements.baml:8:14]
//  8 │   implements Animal {
//    ·              ───┬──
//    ·                 ╰── conflicting implementation is here
//    ╰────
//     ╭─[interface_duplicate_implements.baml:14:14]
//  14 │   implements Animal {
//     ·              ──────
//     ╰────
