client MyClient {
  provider openai
}

//----
//- diagnostics
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client MyClient = openai.OpenAiClient.new(model = "...");` (compose reliability with ai.Retry / ai.Fallback /
//   │ ai.RoundRobin)
//    ╭─[client_missing_template_args.baml:1:1]
//  1 │ ╭─▶ client MyClient {
//  2 │ │     provider openai
//  3 │ ├─▶ }
//    · ╰──── replace with `client Name = <expr>;`
//    ╰────
