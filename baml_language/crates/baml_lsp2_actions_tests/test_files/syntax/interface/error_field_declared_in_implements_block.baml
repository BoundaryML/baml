// E0127: classes satisfy interface fields with class-owned fields, optionally
// linked by `InterfaceField as class_field`. New fields cannot be declared
// inside an `implements` block.

interface Config {
  port: int
}

class Server {
  implements Config {
    port: string
  }
}

//----
//- diagnostics
// Error: field `port` cannot be declared inside `implements Config`
//     ╭─[ interface_error_field_declared_in_implements_block.baml:10:22 ]
//     │
//  10 │ ╭─▶   implements Config {
//  11 │ ├─▶     port: string
//     │ │                      
//     │ ╰────────────────────── add a class field, or link it with `field as class_field`
//     │     
//     │     Note: Error code: E0127
// ────╯
// Error: class `Server` does not provide field `port` required by interface `Config`
//     ╭─[ interface_error_field_declared_in_implements_block.baml:9:15 ]
//     │
//   9 │ ╭─▶ class Server {
//     ┆ ┆   
//  12 │ ├─▶   }
//     │ │         
//     │ ╰───────── add class field `port`, or link it with `port as class_field`
//     │     
//     │     Note: Error code: E0124
// ────╯
