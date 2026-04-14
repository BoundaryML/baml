class Person {
  name string
  age int
}

enum Color {
  R
  G
  B
}

function greet(name: string) -> string {
  let x = "hello";
  x
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- folding_ranges
// basic.baml:1:1-4:2
// basic.baml:6:1-10:2
// basic.baml:12:1-15:2
// basic.baml:12:15-12:29
// basic.baml:12:40-15:2
