client<llm> MyClient {
  provider openai
  options {
    model "gpt-4o"
    client_response_type "invalid"
  }
}

client<llm> MyClient2 {
  provider openai
  options {
    model "gpt-4o"
    client_response_type "openai"
  }
}

client<llm> MyClient3 {
  provider openai
  options {
    model "gpt-4o"
    client_response_type "anthropic"
  }
}

// error: client_response_type must be one of "openai", "openai-responses", "anthropic", "google", or "vertex". Got: invalid
//   -->  client/bad_response_format.baml:5
//    |
//  4 |     model "gpt-4o"
//  5 |     client_response_type "invalid"
//    |

//----
//- diagnostics
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client MyClient = openai.ResponsesClient.new(model = "...");` (compose reliability with ai.Retry / ai.Fallback /
//   │ ai.RoundRobin)
//    ╭─[client_bad_response_format.baml:1:1]
//  1 │ ╭─▶ client<llm> MyClient {
//  2 │ │     provider openai
//  3 │ │     options {
//  4 │ │       model "gpt-4o"
//  5 │ │       client_response_type "invalid"
//  6 │ │     }
//  7 │ ├─▶ }
//    · ╰──── replace with `client Name = <expr>;`
//    ╰────
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client MyClient2 = openai.ResponsesClient.new(model = "...");` (compose reliability with ai.Retry / ai.Fallback /
//   │ ai.RoundRobin)
//     ╭─[client_bad_response_format.baml:9:1]
//   9 │ ╭─▶ client<llm> MyClient2 {
//  10 │ │     provider openai
//  11 │ │     options {
//  12 │ │       model "gpt-4o"
//  13 │ │       client_response_type "openai"
//  14 │ │     }
//  15 │ ├─▶ }
//     · ╰──── replace with `client Name = <expr>;`
//     ╰────
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client MyClient3 = openai.ResponsesClient.new(model = "...");` (compose reliability with ai.Retry / ai.Fallback /
//   │ ai.RoundRobin)
//     ╭─[client_bad_response_format.baml:17:1]
//  17 │ ╭─▶ client<llm> MyClient3 {
//  18 │ │     provider openai
//  19 │ │     options {
//  20 │ │       model "gpt-4o"
//  21 │ │       client_response_type "anthropic"
//  22 │ │     }
//  23 │ ├─▶ }
//     · ╰──── replace with `client Name = <expr>;`
//     ╰────
