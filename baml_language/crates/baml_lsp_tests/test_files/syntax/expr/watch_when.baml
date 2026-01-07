function Top() -> int {
  watch let x = 10;
  x.$watch.options(baml.WatchOptions{ when: GoodFunction });
  x.$watch.options(baml.WatchOptions{ when: BadFunction1 });
  x.$watch.options(baml.WatchOptions{ when: BadFunction2 });
  1
}

function GoodFunction(i: int) -> bool {
  true
}

function BadFunction1(i: int, j: int) -> bool {
  true
}

function BadFunction2(i: string) -> bool {
  true
}

// error: Error validating: Function 'BadFunction2' has incorrect type. Expected (T) -> bool, where T matches the variable type
//   -->  expr/watch_when.baml:5
//    |
//  4 |   x.$watch.options(baml.WatchOptions{ when: BadFunction1 });
//  5 |   x.$watch.options(baml.WatchOptions{ when: BadFunction2 });
//    |
// error: Error validating: Function 'BadFunction1' has incorrect type. Expected (T) -> bool, where T matches the variable type
//   -->  expr/watch_when.baml:4
//    |
//  3 |   x.$watch.options(baml.WatchOptions{ when: GoodFunction });
//  4 |   x.$watch.options(baml.WatchOptions{ when: BadFunction1 });
//    |
// error: Error validating: Function 'BadFunction2' has incorrect type. Expected (T) -> bool, where T matches the variable type
//   -->  expr/watch_when.baml:5
//    |
//  4 |   x.$watch.options(baml.WatchOptions{ when: BadFunction1 });
//  5 |   x.$watch.options(baml.WatchOptions{ when: BadFunction2 });
//    |

//----
//- diagnostics
// Error: Expected `int`, found `int.$watch`
//    ╭─[ expr_watch_when.baml:2:20 ]
//    │
//  2 │ ╭─▶   watch let x = 10;
//  3 │ ├─▶   x.$watch.options(baml.WatchOptions{ when: GoodFunction });
//    │ │                                                                  
//    │ ╰────────────────────────────────────────────────────────────────── Expected `int`, found `int.$watch`
//    │     
//    │     Note: Error code: E0001
// ───╯
// Error: Expected `int`, found `int.$watch`
//    ╭─[ expr_watch_when.baml:3:61 ]
//    │
//  3 │ ╭─▶   x.$watch.options(baml.WatchOptions{ when: GoodFunction });
//  4 │ ├─▶   x.$watch.options(baml.WatchOptions{ when: BadFunction1 });
//    │ │                                                                  
//    │ ╰────────────────────────────────────────────────────────────────── Expected `int`, found `int.$watch`
//    │     
//    │     Note: Error code: E0001
// ───╯
// Error: Expected `int`, found `int.$watch`
//    ╭─[ expr_watch_when.baml:4:61 ]
//    │
//  4 │ ╭─▶   x.$watch.options(baml.WatchOptions{ when: BadFunction1 });
//  5 │ ├─▶   x.$watch.options(baml.WatchOptions{ when: BadFunction2 });
//    │ │                                                                  
//    │ ╰────────────────────────────────────────────────────────────────── Expected `int`, found `int.$watch`
//    │     
//    │     Note: Error code: E0001
// ───╯
