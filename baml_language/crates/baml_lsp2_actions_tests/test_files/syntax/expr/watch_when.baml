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

// Known issue: diagnostic spans for member-access errors extend to the
// preceding statement instead of pointing at the exact member token (`$watch`).
// The spans below are wider than ideal — they should target `x.$watch` only.
// See: https://github.com/BoundaryML/baml/pull/3369#discussion_r3088162711

//----
//- diagnostics
// error: type `int` has no member `$watch`
//    ╭─[ expr_watch_when.baml:2:20 ]
//    │
//  2 │ ╭─▶   watch let x = 10;
//  3 │ ├─▶   x.$watch.options(baml.WatchOptions{ when: GoodFunction });
//    │ │                                                                  
//    │ ╰────────────────────────────────────────────────────────────────── type `int` has no member `$watch`
//    │     
//    │     Note: Error code: E0007
// ───╯
// error: type `int` has no member `$watch`
//    ╭─[ expr_watch_when.baml:3:61 ]
//    │
//  3 │ ╭─▶   x.$watch.options(baml.WatchOptions{ when: GoodFunction });
//  4 │ ├─▶   x.$watch.options(baml.WatchOptions{ when: BadFunction1 });
//    │ │                                                                  
//    │ ╰────────────────────────────────────────────────────────────────── type `int` has no member `$watch`
//    │     
//    │     Note: Error code: E0007
// ───╯
// error: type `int` has no member `$watch`
//    ╭─[ expr_watch_when.baml:4:61 ]
//    │
//  4 │ ╭─▶   x.$watch.options(baml.WatchOptions{ when: BadFunction1 });
//  5 │ ├─▶   x.$watch.options(baml.WatchOptions{ when: BadFunction2 });
//    │ │                                                                  
//    │ ╰────────────────────────────────────────────────────────────────── type `int` has no member `$watch`
//    │     
//    │     Note: Error code: E0007
// ───╯
