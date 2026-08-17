client<l> MyClient2 {
  provider openai
}

// error: Error validating: This line is invalid. It does not start with any known Baml schema keyword.
//   -->  client/bad_template_args.baml:1
//    |
//    |
//  1 | client<l> MyClient2 {
//  2 |   provider openai
//    |

//----
//- diagnostics
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client MyClient2 = openai.ResponsesClient.new(model = "...");` (compose reliability with ai.Retry / ai.Fallback /
//   │ ai.RoundRobin)
//    ╭─[client_bad_template_args.baml:1:1]
//  1 │ ╭─▶ client<l> MyClient2 {
//  2 │ │     provider openai
//  3 │ ├─▶ }
//    · ╰──── replace with `client Name = <expr>;`
//    ╰────
