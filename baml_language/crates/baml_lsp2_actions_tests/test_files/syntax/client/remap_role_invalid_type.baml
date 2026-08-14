client<llm> MyClient {
  provider openai
  options {
    model "gpt-4o"
    remap_roles "not_a_map"
  }
}

// error: remap_roles must be a map. Got: string
//   -->  client/remap_role_invalid_type.baml:5
//    |
//  4 |     model "gpt-4o"
//  5 |     remap_roles "not_a_map"
//    |

//----
//- diagnostics
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client MyClient = openai.OpenAiClient.new(model = "...");` (compose reliability with ai.Retry / ai.Fallback /
//   │ ai.RoundRobin)
//    ╭─[client_remap_role_invalid_type.baml:1:1]
//  1 │ ╭─▶ client<llm> MyClient {
//  2 │ │     provider openai
//  3 │ │     options {
//  4 │ │       model "gpt-4o"
//  5 │ │       remap_roles "not_a_map"
//  6 │ │     }
//  7 │ ├─▶ }
//    · ╰──── replace with `client Name = <expr>;`
//    ╰────
