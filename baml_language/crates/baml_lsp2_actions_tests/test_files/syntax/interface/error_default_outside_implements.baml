// BEP-044: `default.method()` is only legal inside an `implements` block.

interface Logger {
  function log(self, msg: string) -> string {
    return "[LOG] " + msg
  }
}

class Plain {
  function bare(self) -> string {
    return default.log("hi")
  }
}

//----
//- diagnostics
// error: unresolved name: default
//     ╭─[ interface_error_default_outside_implements.baml:11:12 ]
//     │
//  11 │     return default.log("hi")
//     │            ─────┬─────
//     │                 ╰─────── unresolved name: default
//     │
//     │ Note: Error code: E0003
// ────╯
