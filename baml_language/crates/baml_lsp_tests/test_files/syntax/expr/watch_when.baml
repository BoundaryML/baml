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
// Error: Expected expression, found Watch
//    ╭─[ 0:2:3 ]
//    │
//  2 │   watch let x = 10;
//    │   ──┬──  
//    │     ╰──── Expected expression, found Watch
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Expected field name after '.', found Dollar
//    ╭─[ 0:3:5 ]
//    │
//  3 │   x.$watch.options(baml.WatchOptions{ when: GoodFunction });
//    │     ┬  
//    │     ╰── Expected Expected field name after '.', found Dollar
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Expected field name after '$', found Watch
//    ╭─[ 0:3:6 ]
//    │
//  3 │   x.$watch.options(baml.WatchOptions{ when: GoodFunction });
//    │      ──┬──  
//    │        ╰──── Expected Expected field name after '$', found Watch
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected expression, found Watch
//    ╭─[ 0:3:6 ]
//    │
//  3 │   x.$watch.options(baml.WatchOptions{ when: GoodFunction });
//    │      ──┬──  
//    │        ╰──── Expected expression, found Watch
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Expected field name after '.', found Dollar
//    ╭─[ 0:4:5 ]
//    │
//  4 │   x.$watch.options(baml.WatchOptions{ when: BadFunction1 });
//    │     ┬  
//    │     ╰── Expected Expected field name after '.', found Dollar
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Expected field name after '$', found Watch
//    ╭─[ 0:4:6 ]
//    │
//  4 │   x.$watch.options(baml.WatchOptions{ when: BadFunction1 });
//    │      ──┬──  
//    │        ╰──── Expected Expected field name after '$', found Watch
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected expression, found Watch
//    ╭─[ 0:4:6 ]
//    │
//  4 │   x.$watch.options(baml.WatchOptions{ when: BadFunction1 });
//    │      ──┬──  
//    │        ╰──── Expected expression, found Watch
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Expected field name after '.', found Dollar
//    ╭─[ 0:5:5 ]
//    │
//  5 │   x.$watch.options(baml.WatchOptions{ when: BadFunction2 });
//    │     ┬  
//    │     ╰── Expected Expected field name after '.', found Dollar
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Expected field name after '$', found Watch
//    ╭─[ 0:5:6 ]
//    │
//  5 │   x.$watch.options(baml.WatchOptions{ when: BadFunction2 });
//    │      ──┬──  
//    │        ╰──── Expected Expected field name after '$', found Watch
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected expression, found Watch
//    ╭─[ 0:5:6 ]
//    │
//  5 │   x.$watch.options(baml.WatchOptions{ when: BadFunction2 });
//    │      ──┬──  
//    │        ╰──── Expected expression, found Watch
//    │ 
//    │ Note: Error code: E0010
// ───╯
