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
// Error: unreachable arm
//      ╭─[ llm_types.baml:159:17 ]
//      │
//  159 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:189:17 ]
//      │
//  189 │ ╭─▶             _ => {
//      ┆ ┆   
//  194 │ ├─▶             }
//      │ │                   
//      │ ╰─────────────────── unreachable arm
//      │     
//      │     Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:208:17 ]
//      │
//  208 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
