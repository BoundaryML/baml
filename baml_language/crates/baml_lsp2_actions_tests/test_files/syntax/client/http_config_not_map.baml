client<llm> HttpNotMap {
  provider openai
  options {
    model "gpt-4"
    http "invalid"  // Should be a map
  }
}

// error: http must be a configuration block with timeout settings
//   -->  client/http_config_not_map.baml:5
//    |
//  4 |     model "gpt-4"
//  5 |     http "invalid"  // Should be a map
//    |

//----
//- diagnostics
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client HttpNotMap = openai.ResponsesClient.new(model = "...");` (compose reliability with ai.Retry / ai.Fallback /
//   │ ai.RoundRobin)
//    ╭─[client_http_config_not_map.baml:1:1]
//  1 │ ╭─▶ client<llm> HttpNotMap {
//  2 │ │     provider openai
//  3 │ │     options {
//  4 │ │       model "gpt-4"
//  5 │ │       http "invalid"  // Should be a map
//  6 │ │     }
//  7 │ ├─▶ }
//    · ╰──── replace with `client Name = <expr>;`
//    ╰────
