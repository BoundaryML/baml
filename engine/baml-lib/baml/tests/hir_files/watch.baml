function Foo() -> int {
  watch let x = 5;
  watch let y = 10;
  watch let z: int = 20;
  z.$watch.options(baml.WatchOptions { when: MyFunctionGood });
  z.$watch.options(baml.WatchOptions { when: MyFunctionBad });
  10
}


function MyFunctionBad(value: int) -> bool {
  true
}

function MyFunctionGood(value: int) -> bool {
  true
}

// function Foo() {
//   let x = 5 @watch(name=x);
//   let y = 10 @watch(name=y);
//   let z: int = 20 @watch(when=MyFunctionBad, name=z);
//   z.$watch.options(when: FunctionName(Local("MyFunctionGood", Span { file: SourceFile { path: "test.baml", contents: ... }, start: 133, end: 147 })));
//   z.$watch.options(when: FunctionName(Local("MyFunctionBad", Span { file: SourceFile { path: "test.baml", contents: ... }, start: 197, end: 210 })));
//
//   10
// }
//
// function MyFunctionBad(value) {
//   true
// }
//
// function MyFunctionGood(value) {
//   true
// }
