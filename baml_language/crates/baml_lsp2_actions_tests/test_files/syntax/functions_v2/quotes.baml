class Foo {
  name string
}

function Bar(foo: Foo) -> Foo {
  client: "openai/gpt-4o"
  prompt: `
    ${ctx.output_format()}
  `
}

//----
//- diagnostics
// <no-diagnostics-expected>
