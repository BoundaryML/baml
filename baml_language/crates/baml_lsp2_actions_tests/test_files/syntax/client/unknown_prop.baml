client<llm> MyClient {
  provider openai
  myExtraProp "hello"
  options {
    max_tokens 100
  }
}

// error: Error validating: Unknown field `myExtraProp` in client
//   -->  client/unknown_prop.baml:3
//    |
//  2 |   provider openai
//  3 |   myExtraProp "hello"
//    |

//----
//- diagnostics
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client MyClient = openai.ResponsesClient.new(model = "...");` (compose reliability with ai.Retry / ai.Fallback /
//   │ ai.RoundRobin)
//    ╭─[client_unknown_prop.baml:1:1]
//  1 │ ╭─▶ client<llm> MyClient {
//  2 │ │     provider openai
//  3 │ │     myExtraProp "hello"
//  4 │ │     options {
//  5 │ │       max_tokens 100
//  6 │ │     }
//  7 │ ├─▶ }
//    · ╰──── replace with `client Name = <expr>;`
//    ╰────
