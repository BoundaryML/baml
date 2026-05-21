// BEP-044: old `obj.Interface.method()` projection syntax is rejected. Use
// `obj.as<Interface>.method()` instead.

interface Animal {
  function speak(self) -> string
}

class Dog {
  implements Animal {
    function speak(self) -> string { return "Woof!" }
  }
}

function bad() -> string {
  let d = Dog {}
  return d.Animal.speak()
}

//----
//- diagnostics
// Error: interface projection uses `.as<Animal>`, not `.Animal`
//     ╭─[ interface_error_old_qualified_projection.baml:16:9 ]
//     │
//  16 │   return d.Animal.speak()
//     │         ───────┬───────  
//     │                ╰───────── interface projection uses `.as<Animal>`, not `.Animal`
//     │ 
//     │ Note: Error code: E0007
// ────╯
