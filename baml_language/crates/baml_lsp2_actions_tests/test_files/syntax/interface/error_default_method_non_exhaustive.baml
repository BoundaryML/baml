interface Labels {
  function label(self, value: bool) -> string throws never {
    return match (value) {
      true => "yes"
    }
  }
}

class Thing {
  implements Labels {}
}

//----
//- diagnostics
// E0062
//
//   × non-exhaustive match on type bool; missing: false
//    ╭─[interface_error_default_method_non_exhaustive.baml:3:12]
//  3 │ ╭─▶     return match (value) {
//  4 │ │         true => "yes"
//  5 │ ╰─▶     }
//    ╰────
