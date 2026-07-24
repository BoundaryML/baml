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
// E0127
//
//   × field `port` cannot be declared inside `implements Config`
//     ╭─[interface_error_field_declared_in_implements_block.baml:11:5]
//  11 │     port: string
//     ·     ──────┬─────
//     ·           ╰── add a class field, or link it with `field as class_field`
//     ╰────
// E0124
//
//   × missing field `port` required by interface `Config` (add a class field named `port` or link one with `port as <class_field>`)
//     ╭─[interface_error_field_declared_in_implements_block.baml:10:14]
//  10 │   implements Config {
//     ·              ──────
//     ╰────
