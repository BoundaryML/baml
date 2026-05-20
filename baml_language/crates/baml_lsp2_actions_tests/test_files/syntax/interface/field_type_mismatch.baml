// `port` is declared as `string` on the class but `int` on the interface.

interface Config {
  host: string
  port: int
}

class Server {
  implements Config {
    host: string
    port: string
  }
}

//----
//- diagnostics
// Error: class `Server` declares field `port: string` but interface `Config` requires `port: int`
//     ╭─[ interface_field_type_mismatch.baml:11:10 ]
//     │
//  11 │     port: string
//     │          ───┬───  
//     │             ╰───── expected `int`, found `string`
//     │ 
//     │ Note: Error code: E0116
// ────╯
