// `implements` names something that isn't a known interface.

class Ghost {
  implements DoesNotExist {}
}

//----
//- diagnostics
// error: class `Ghost` cannot implement `DoesNotExist`: no interface with that name is in scope
//    ╭─[ interface_unknown_interface.baml:4:14 ]
//    │
//  4 │   implements DoesNotExist {}
//    │              ──────┬─────
//    │                    ╰─────── interface not found
//    │
//    │ Note: Error code: E0112
// ───╯
