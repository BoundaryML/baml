// BEP-044: methods with additional `Self` parameters need a concrete receiver.

interface Equatable {
  function same(self, other: Self) -> bool
}

class Box {
  value: int

  implements Equatable {
    function same(self, other: Self) -> bool {
      return self.value == other.value
    }
  }
}

function bad(a: Equatable, b: Equatable) -> bool {
  return a.same(b)
}

//----
//- diagnostics
// error: method `same` on interface `Equatable` uses `Self` in its parameters and requires a concrete receiver
//     ╭─[ interface_error_multi_self_call_on_interface_receiver.baml:18:9 ]
//     │
//  18 │   return a.same(b)
//     │         ───┬───  
//     │            ╰───── method `same` on interface `Equatable` uses `Self` in its parameters and requires a concrete receiver
//     │ 
//     │ Note: Error code: E0001
// ────╯
