client<llm> Hello {
  provider openai
  options {
    thing hello'world
    banned @helloworld
    banned2 #helloworld
    banned3 hello(world)
  }
}

// error: Error validating: This line is invalid. It does not start with any known Baml schema keyword.
//   -->  strings/unquoted_strings.baml:1
//    |
//    |
//  1 | client<llm> Hello {
//  2 |   provider openai
//    |

//----
//- diagnostics
// E0010
//
//   × unexpected token
//    ╭─[strings_unquoted_strings.baml:4:16]
//  4 │     thing hello'world
//    ·                ┬
//    ·                ╰── expected `config key`, found `error`
//    ╰────
// E0010
//
//   × unexpected token
//    ╭─[strings_unquoted_strings.baml:6:13]
//  6 │     banned2 #helloworld
//    ·             ┬
//    ·             ╰── expected `identifier`, found `'#'`
//    ╰────
// E0010
//
//   × unexpected token
//    ╭─[strings_unquoted_strings.baml:7:18]
//  7 │     banned3 hello(world)
//    ·                  ┬
//    ·                  ╰── expected `identifier`, found `'('`
//    ╰────
// E0010
//
//   × unexpected token
//    ╭─[strings_unquoted_strings.baml:7:24]
//  7 │     banned3 hello(world)
//    ·                        ┬
//    ·                        ╰── expected `identifier`, found `')'`
//    ╰────
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client Hello = openai.OpenAiClient.new(model = "...");` (compose reliability with ai.Retry / ai.Fallback /
//   │ ai.RoundRobin)
//    ╭─[strings_unquoted_strings.baml:1:1]
//  1 │ ╭─▶ client<llm> Hello {
//  2 │ │     provider openai
//  3 │ │     options {
//  4 │ │       thing hello'world
//  5 │ │       banned @helloworld
//  6 │ │       banned2 #helloworld
//  7 │ │       banned3 hello(world)
//  8 │ │     }
//  9 │ ├─▶ }
//    · ╰──── replace with `client Name = <expr>;`
//    ╰────
