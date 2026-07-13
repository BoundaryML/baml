// `port` is declared as `string` on the class but `int` on the interface.

interface Config {
  host: string
  port: int
}

class Server {
  host: string
  port: string

  implements Config {}
}

//----
//- diagnostics
// error: field `port` has type string, but interface `Config` declares it as int
//     ╭─[ interface_field_type_mismatch.baml:12:14 ]
//     │
//  12 │   implements Config {}
//     │              ───┬──
//     │                 ╰──── field `port` has type string, but interface `Config` declares it as int
//     │
//     │ Note: Error code: E0116
// ────╯
