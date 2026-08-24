client Bar = openai.ResponsesClient.new(model = "gpt-4o");

function Foo() -> string {
  `This! ${b}`
}

function Foo2() -> string {
  `Also This!`
}

function FooBar(a: string, b: int | bool) -> int {
  client: Bar
  prompt: `
    fa ${a}
  `
}


function FooBar1(a: string, b: int | bool) -> int {
  client: Bar
  prompt: `
    fa ${a}
  `
}

function Bar1(a: string) -> int {
  client: Bar
  prompt: `
    fa ${a}
  `
}

//----
//- diagnostics
// E0003
//
//   × unresolved name: `b`
//    ╭─[functions_v2_prompt_errors_prompt1.baml:4:12]
//  4 │   `This! ${b}`
//    ·            ─
//    ╰────
