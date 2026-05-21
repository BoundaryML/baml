// `speak()` has no default body — class must provide one.

interface Animal {
  function speak(self) -> string
}

class IncompleteAnimal {
  implements Animal {}
}

//----
//- diagnostics
// error: class `IncompleteAnimal` does not implement required method `speak` of interface `Animal`
//    ╭─[ interface_missing_required_method.baml:7:25 ]
//    │
//  7 │ ╭─▶ class IncompleteAnimal {
//  8 │ ├─▶   implements Animal {}
//    │ │                            
//    │ ╰──────────────────────────── missing `speak` here
//    │     
//    │     Note: Error code: E0113
// ───╯
