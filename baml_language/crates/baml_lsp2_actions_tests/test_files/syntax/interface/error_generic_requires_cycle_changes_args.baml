interface Loop<T> requires Loop<T[]> {
  function ping(self) -> string throws never
}

class Box {
  implements Loop<int> {
    function ping(self) -> string {
      return "ok"
    }
  }
}

//----
//- diagnostics
// E0125
//
//   × implementing `Loop` also requires implementing `Loop<int[]>`
//    ╭─[interface_error_generic_requires_cycle_changes_args.baml:6:14]
//  6 │   implements Loop<int> {
//    ·              ─────────
//    ╰────
