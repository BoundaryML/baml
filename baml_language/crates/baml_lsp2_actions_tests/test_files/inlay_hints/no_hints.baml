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

// client declarations are desugared; no hints should appear
client GPT4 = openai.OpenAiClient.new(model = "gpt-4");

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- inlay_hints
// <no-inlay-hints>
