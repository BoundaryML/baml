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
// error: class `Server` declares field `port: string` but interface `Config` requires `port` to have type `int`
//     ╭─[ interface_field_type_mismatch.baml:10:9 ]
//     │
//  10 │   port: string
//     │         ───┬──
//     │            ╰──── expected `int`, found `string`
//     │
//     │ Note: Error code: E0116
// ────╯
