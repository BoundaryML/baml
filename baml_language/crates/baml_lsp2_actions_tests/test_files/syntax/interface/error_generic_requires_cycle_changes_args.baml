interface Loop<T> requires Loop<T[]> {
  function ping(self) -> string
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
// error: interface `requires` chain forms a cycle: Loop
//    ╭─[ interface_error_generic_requires_cycle_changes_args.baml:1:11 ]
//    │
//  1 │ interface Loop<T> requires Loop<T[]> {
//    │           ──┬─  
//    │             ╰─── cyclic `requires`
//    │ 
//    │ Note: Error code: E0118
// ───╯
// error: class `Box` implements `user.Loop`, which requires `user.Loop`, but `Box` does not implement them
//    ╭─[ interface_error_generic_requires_cycle_changes_args.baml:6:13 ]
//    │
//  6 │   implements Loop<int> {
//    │             ─────┬────  
//    │                  ╰────── requires `user.Loop`
//    │ 
//    │ Note: Error code: E0125
// ───╯
