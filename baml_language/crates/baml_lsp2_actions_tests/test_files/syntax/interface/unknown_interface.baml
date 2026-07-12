// `implements` names something that isn't a known interface.

class Ghost {
  implements DoesNotExist {}
}

//----
//- diagnostics
// error: unresolved type: DoesNotExist
//    ╭─[ interface_unknown_interface.baml:4:14 ]
//    │
//  4 │   implements DoesNotExist {}
//    │              ──────┬─────
//    │                    ╰─────── unresolved type: DoesNotExist
//    │
//    │ Note: Error code: E0002
// ───╯
