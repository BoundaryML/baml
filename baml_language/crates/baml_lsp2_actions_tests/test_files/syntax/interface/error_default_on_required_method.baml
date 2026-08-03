// BEP-044: `default.method()` is invalid when the interface's method has no
// default body (it is a required method).

interface Animal {
  function speak(self) -> string throws never
}

class Dog {
  implements Animal {
    function speak(self) -> string {
      return default.speak()
    }
  }
}

//----
//- diagnostics
// E0123
//
//   × `default.speak()` is invalid: method `speak` on interface `Animal` has no default body
//     ╭─[interface_error_default_on_required_method.baml:11:14]
//  11 │       return default.speak()
//     ·              ─────────────
//     ╰────
