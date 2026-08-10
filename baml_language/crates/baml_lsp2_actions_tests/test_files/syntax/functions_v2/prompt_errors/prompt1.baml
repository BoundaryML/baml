client Bar = openai.OpenAiClient.new(model = "gpt-4o");

template_string Foo() #"
  This! {{ b}}
"#

template_string Foo2() #"
  Also This!
"#

function FooBar(a: string, b: int | bool) -> int {
  client Bar
  prompt `
    fa ${a}
  `
}


function FooBar1(a: string, b: int | bool) -> int {
  client Bar
  prompt `
    fa ${a}
  `
}

function Bar1(a: string) -> int {
  client Bar
  prompt `
    fa ${a}
  `
}

//----
//- diagnostics
// E0070
//
//   ⚠ Variable `b` does not exist. Did you mean one of these: `_`, `baml`, `ctx`?
//    ╭─[functions_v2_prompt_errors_prompt1.baml:4:12]
//  4 │   This! {{ b}}
//    ·            ─
//    ╰────
