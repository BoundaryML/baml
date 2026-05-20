// E0124: implements block is missing required field `port` from interface Config.

interface Config {
    host: string
    port: int
}

class Server {
    implements Config {
        host: string
    }
}

//----
//- diagnostics
// Error: class `Server` does not redeclare required field `port` of interface `Config` in its `implements` block
//     ╭─[ interface_error_missing_field_in_implements_block.baml:8:15 ]
//     │
//   8 │ ╭─▶ class Server {
//     ┆ ┆   
//  11 │ ├─▶     }
//     │ │           
//     │ ╰─────────── missing `port` here
//     │     
//     │     Note: Error code: E0124
// ────╯
