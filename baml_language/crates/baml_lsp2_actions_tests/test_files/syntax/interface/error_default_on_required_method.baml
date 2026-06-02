// BEP-044: `default.method()` is invalid when the interface's method has no
// default body (it is a required method).

interface Animal {
  function speak(self) -> string
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
// error: `default.speak()` is invalid: method `speak` on interface `Animal` has no default body
//     ╭─[ interface_error_default_on_required_method.baml:11:13 ]
//     │
//  11 │       return default.speak()
//     │             ───────┬──────  
//     │                    ╰──────── `default.speak()` is invalid: method `speak` on interface `Animal` has no default body
//     │ 
//     │ Note: Error code: E0123
// ────╯
