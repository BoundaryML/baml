// `speak()` has no default body — class must provide one.

interface Animal {
  function speak(self) -> string throws never
}

class IncompleteAnimal {
  implements Animal {}
}

//----
//- diagnostics
// error: missing implementation of method `speak` required by interface `Animal`
//    ╭─[ interface_missing_required_method.baml:8:14 ]
//    │
//  8 │   implements Animal {}
//    │              ───┬──
//    │                 ╰──── missing implementation of method `speak` required by interface `Animal`
//    │
//    │ Note: Error code: E0113
// ───╯
