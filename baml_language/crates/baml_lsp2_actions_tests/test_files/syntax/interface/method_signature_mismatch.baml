// The class declares `speak(self) -> int` but the interface requires
// `speak(self) -> string`.

interface Animal {
  function speak(self) -> string throws never
}

class Robot {
  implements Animal {
    function speak(self) -> int {
      return 42
    }
  }
}

//----
//- diagnostics
// E0120
//
//   × method `speak` has signature (self: Robot) -> int throws never, which does not conform to interface `Animal`'s declared (self: Robot) -> string throws never
//     ╭─[interface_method_signature_mismatch.baml:10:5]
//  10 │ ╭─▶     function speak(self) -> int {
//  11 │ │         return 42
//  12 │ ╰─▶     }
//     ╰────
