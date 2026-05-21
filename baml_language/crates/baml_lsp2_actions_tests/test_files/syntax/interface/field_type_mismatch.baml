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
// Error: class `Server` declares field `port: string` but interface `Config` requires `port: int`
//     ╭─[ interface_field_type_mismatch.baml:10:8 ]
//     │
//  10 │   port: string
//     │        ───┬───  
//     │           ╰───── expected `int`, found `string`
//     │ 
//     │ Note: Error code: E0116
// ────╯
