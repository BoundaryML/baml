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
// E0118
//
//   × interface `requires` cycle: Loop → Loop
//    ╭─[interface_error_generic_requires_cycle_changes_args.baml:1:1]
//  1 │ ╭─▶ interface Loop<T> requires Loop<T[]> {
//  2 │ │     function ping(self) -> string throws never
//  3 │ ╰─▶ }
//    ╰────
// E0125
//
//   × implementing `Loop` also requires implementing `Loop<int[]>`
//    ╭─[interface_error_generic_requires_cycle_changes_args.baml:6:14]
//  6 │   implements Loop<int> {
//    ·              ─────────
//    ╰────
