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
// error: interface `requires` cycle: Loop → Loop
//    ╭─[ interface_error_generic_requires_cycle_changes_args.baml:1:1 ]
//    │
//  1 │ ╭─▶ interface Loop<T> requires Loop<T[]> {
//    ┆ ┆
//  3 │ ├─▶ }
//    │ │
//    │ ╰─────── interface `requires` cycle: Loop → Loop
//    │
//    │     Note: Error code: E0118
// ───╯
// error: implementing `Loop` also requires implementing `Loop<int[]>`
//    ╭─[ interface_error_generic_requires_cycle_changes_args.baml:6:14 ]
//    │
//  6 │   implements Loop<int> {
//    │              ────┬────
//    │                  ╰────── implementing `Loop` also requires implementing `Loop<int[]>`
//    │
//    │ Note: Error code: E0125
// ───╯
