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
// E0124
//
//   × missing field `port` required by interface `Config` (add a class field named `port` or link one with `port as <class_field>`)
//     ╭─[interface_error_missing_field_in_implements_block.baml:11:16]
//  11 │     implements Config {}
//     ·                ──────
//     ╰────
