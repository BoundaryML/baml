// BEP-044: methods with additional `Self` parameters need a concrete receiver.

interface Equatable {
  function same(self, other: Self) -> bool throws never
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
// E0001
//
//   × method `same` on interface `Equatable` uses `Self` in a parameter, so it requires a concrete receiver, not an interface-existential one
//     ╭─[interface_error_multi_self_call_on_interface_receiver.baml:18:10]
//  18 │   return a.same(b)
//     ·          ──────
//     ╰────
