// E0124: class is missing required field `port` from interface Config.

interface Config {
    host: string
    port: int
}

class Server {
    host: string

    implements Config {}
}

//----
//- diagnostics
// error: class `Server` does not provide field `port` required by interface `user.Config`
//     ╭─[ interface_error_missing_field_in_implements_block.baml:9:17 ]
//     │
//   9 │ ╭─▶     host: string
//     ┆ ┆   
//  11 │ ├─▶     implements Config {}
//     │ │                              
//     │ ╰────────────────────────────── add class field `port`, or link it with `port as class_field`
//     │     
//     │     Note: Error code: E0124
// ────╯
