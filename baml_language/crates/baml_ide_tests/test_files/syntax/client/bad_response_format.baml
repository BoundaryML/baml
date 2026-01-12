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
// Error: client_response_type must be one of openai, openai-responses, anthropic, google, vertex, openrouter. Got: invalid
//    ╭─[ client_bad_response_format.baml:5:25 ]
//    │
//  5 │     client_response_type "invalid"
//    │                         ─────┬────  
//    │                              ╰────── client_response_type must be one of openai, openai-responses, anthropic, google, vertex, openrouter. Got: invalid
//    │ 
//    │ Note: Error code: E0022
// ───╯
