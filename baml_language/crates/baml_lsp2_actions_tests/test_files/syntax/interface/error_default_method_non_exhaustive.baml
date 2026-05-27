interface Labels {
  function label(self, value: bool) -> string {
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
// error: non-exhaustive match on type bool; missing: false
//    ╭─[ interface_error_default_method_non_exhaustive.baml:3:11 ]
//    │
//  3 │ ╭─▶     return match (value) {
//    ┆ ┆   
//  5 │ ├─▶     }
//    │ │           
//    │ ╰─────────── non-exhaustive match on type bool; missing: false
//    │     
//    │     Note: Error code: E0062
// ───╯
