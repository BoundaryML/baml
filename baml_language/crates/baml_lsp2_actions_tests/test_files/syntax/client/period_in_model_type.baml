client<llm> MyClient {
  provider "openai"
  options {
    model "gpt-3.5-turbo"
    max_tokens 100
  }
}

//----
//- diagnostics
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client MyClient = openai.ResponsesClient.new(model = "...");` (compose reliability with ai.Retry / ai.Fallback /
//   │ ai.RoundRobin)
//    ╭─[client_period_in_model_type.baml:1:1]
//  1 │ ╭─▶ client<llm> MyClient {
//  2 │ │     provider "openai"
//  3 │ │     options {
//  4 │ │       model "gpt-3.5-turbo"
//  5 │ │       max_tokens 100
//  6 │ │     }
//  7 │ ├─▶ }
//    · ╰──── replace with `client Name = <expr>;`
//    ╰────
