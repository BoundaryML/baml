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
// E0003
//
//   × unresolved name: `default.speak`
//     ╭─[interface_error_default_on_required_method.baml:11:14]
//  11 │       return default.speak()
//     ·              ─────────────
//     ╰────
