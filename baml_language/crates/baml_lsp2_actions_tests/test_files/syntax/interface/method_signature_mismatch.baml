// The class declares `speak(self) -> int` but the interface requires
// `speak(self) -> string`.

interface Animal {
  function speak(self) -> string
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
// error: method `speak` in class `Robot` does not match interface `user.Animal`: expected `() -> string`, found `() -> int`
//     ╭─[ interface_method_signature_mismatch.baml:10:14 ]
//     │
//  10 │     function speak(self) -> int {
//     │              ──┬──  
//     │                ╰──── signature mismatch
//     │ 
//     │ Note: Error code: E0120
// ────╯
