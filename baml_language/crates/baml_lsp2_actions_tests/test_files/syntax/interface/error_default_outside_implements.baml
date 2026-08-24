// BEP-044: `default.method()` is only legal inside an `implements` block.

interface Logger {
  function log(self, msg: string) -> string throws never {
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
// E0003
//
//   × unresolved name: `default.log`
//     ╭─[interface_error_default_outside_implements.baml:11:12]
//  11 │     return default.log("hi")
//     ·            ───────────
//     ╰────
