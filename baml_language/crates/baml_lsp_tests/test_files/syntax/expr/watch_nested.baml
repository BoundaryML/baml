function WorkflowWatch() -> int {
  watch let x: int = 10;
  watch let y: bool = true;

  y = false;
  x = WorkflowWatchChild();
  x
}

function WorkflowWatchChild() -> int {
  watch let x: string = "Hello";
  100
}

//----
//- diagnostics
// Error: Expected expression, found Watch
//    ╭─[ 0:2:3 ]
//    │
//  2 │   watch let x: int = 10;
//    │   ──┬──  
//    │     ╰──── Expected expression, found Watch
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected expression, found Watch
//    ╭─[ 0:3:3 ]
//    │
//  3 │   watch let y: bool = true;
//    │   ──┬──  
//    │     ╰──── Expected expression, found Watch
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected expression, found Watch
//     ╭─[ 0:11:3 ]
//     │
//  11 │   watch let x: string = "Hello";
//     │   ──┬──  
//     │     ╰──── Expected expression, found Watch
//     │ 
//     │ Note: Error code: E0010
// ────╯
