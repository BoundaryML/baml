// `implements` names something that isn't a known interface.

class Ghost {
  implements DoesNotExist {}
}

//----
//- diagnostics
// E0002
//
//   × unresolved type: DoesNotExist
//    ╭─[interface_unknown_interface.baml:4:14]
//  4 │   implements DoesNotExist {}
//    ·              ────────────
//    ╰────
