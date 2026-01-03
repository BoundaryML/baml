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
// Error: Expected Let, found Dot
//    ╭─[ 0:3:11 ]
//    │
//  3 │   x.$watch.options(baml.WatchOptions{ when: GoodFunction });
//    │           ┬  
//    │           ╰── Expected Let, found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected variable name, found Dot
//    ╭─[ 0:3:11 ]
//    │
//  3 │   x.$watch.options(baml.WatchOptions{ when: GoodFunction });
//    │           ┬  
//    │           ╰── Expected variable name, found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected initializer (=), found Dot
//    ╭─[ 0:3:11 ]
//    │
//  3 │   x.$watch.options(baml.WatchOptions{ when: GoodFunction });
//    │           ┬  
//    │           ╰── Expected initializer (=), found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected expression, found Dot
//    ╭─[ 0:3:11 ]
//    │
//  3 │   x.$watch.options(baml.WatchOptions{ when: GoodFunction });
//    │           ┬  
//    │           ╰── Expected expression, found Dot
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
// Error: Expected Let, found Dot
//    ╭─[ 0:4:11 ]
//    │
//  4 │   x.$watch.options(baml.WatchOptions{ when: BadFunction1 });
//    │           ┬  
//    │           ╰── Expected Let, found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected variable name, found Dot
//    ╭─[ 0:4:11 ]
//    │
//  4 │   x.$watch.options(baml.WatchOptions{ when: BadFunction1 });
//    │           ┬  
//    │           ╰── Expected variable name, found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected initializer (=), found Dot
//    ╭─[ 0:4:11 ]
//    │
//  4 │   x.$watch.options(baml.WatchOptions{ when: BadFunction1 });
//    │           ┬  
//    │           ╰── Expected initializer (=), found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected expression, found Dot
//    ╭─[ 0:4:11 ]
//    │
//  4 │   x.$watch.options(baml.WatchOptions{ when: BadFunction1 });
//    │           ┬  
//    │           ╰── Expected expression, found Dot
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
// Error: Expected Let, found Dot
//    ╭─[ 0:5:11 ]
//    │
//  5 │   x.$watch.options(baml.WatchOptions{ when: BadFunction2 });
//    │           ┬  
//    │           ╰── Expected Let, found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected variable name, found Dot
//    ╭─[ 0:5:11 ]
//    │
//  5 │   x.$watch.options(baml.WatchOptions{ when: BadFunction2 });
//    │           ┬  
//    │           ╰── Expected variable name, found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected initializer (=), found Dot
//    ╭─[ 0:5:11 ]
//    │
//  5 │   x.$watch.options(baml.WatchOptions{ when: BadFunction2 });
//    │           ┬  
//    │           ╰── Expected initializer (=), found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected expression, found Dot
//    ╭─[ 0:5:11 ]
//    │
//  5 │   x.$watch.options(baml.WatchOptions{ when: BadFunction2 });
//    │           ┬  
//    │           ╰── Expected expression, found Dot
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Unknown variable options
//    ╭─[ 0:3:12 ]
//    │
//  3 │   x.$watch.options(baml.WatchOptions{ when: GoodFunction });
//    │            ───┬───  
//    │               ╰───── Unknown variable options
//    │ 
//    │ Note: Error code: E0003
// ───╯
// Error: Unknown variable options
//    ╭─[ 0:4:12 ]
//    │
//  4 │   x.$watch.options(baml.WatchOptions{ when: BadFunction1 });
//    │            ───┬───  
//    │               ╰───── Unknown variable options
//    │ 
//    │ Note: Error code: E0003
// ───╯
// Error: Unknown variable options
//    ╭─[ 0:5:12 ]
//    │
//  5 │   x.$watch.options(baml.WatchOptions{ when: BadFunction2 });
//    │            ───┬───  
//    │               ╰───── Unknown variable options
//    │ 
//    │ Note: Error code: E0003
// ───╯
