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
