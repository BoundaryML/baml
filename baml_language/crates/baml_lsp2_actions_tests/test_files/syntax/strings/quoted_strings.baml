client<llm> Hello {
  provider "openai" // Trailing chat
  options {
    thing "word with \"quotes\""
  }
}

//----
//- diagnostics
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client Hello = openai.ResponsesClient.new(model = "...");` (compose reliability with ai.Retry / ai.Fallback /
//   │ ai.RoundRobin)
//    ╭─[strings_quoted_strings.baml:1:1]
//  1 │ ╭─▶ client<llm> Hello {
//  2 │ │     provider "openai" // Trailing chat
//  3 │ │     options {
//  4 │ │       thing "word with \"quotes\""
//  5 │ │     }
//  6 │ ├─▶ }
//    · ╰──── replace with `client Name = <expr>;`
//    ╰────
