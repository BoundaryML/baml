class Input {
  str_field string
}

function Foo(input: Input[]) -> string {
  client: "openai/gpt-4o-mini"
  prompt: `Tell me a haiku.`
}

test Foo {
  functions [Foo]
  input [
    {
      str_field "hello"
    },,
  ]
}

//----
//- diagnostics
// E0010
//
//   × unexpected token
//     ╭─[misc_bad_syntax.baml:15:7]
//  15 │     },,
//     ·       ┬
//     ·       ╰── expected `identifier`, found `','`
//     ╰────
