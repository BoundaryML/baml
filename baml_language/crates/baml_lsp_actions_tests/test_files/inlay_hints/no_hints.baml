class Person {
  name string
  age int
}

enum Color {
  R
  G
  B
}

class Dog {}

class Cat {}

class Fish {}

type Pet = Dog | Cat | Fish;

function foo() -> string {
  // no inlay hint when there is explicit type annotation
  let x: string = "hello";
  x
}

// client blocks are desugared into functions; no hints should appear
client<llm> GPT4 {
    provider openai
    options {
        model "gpt-4"
    }
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- inlay_hints
// <no-inlay-hints>
