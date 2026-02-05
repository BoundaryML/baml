client<llm> MyClient {
  options {
    max_tokens 100
  }
}

// error: Error validating: Missing `provider` field in client. e.g. `provider openai`
//   -->  client/required_provider.baml:1
//    |
//    |
//  1 | client<llm> MyClient {
//  2 |   options {
//  3 |     max_tokens 100
//  4 |   }
//  5 | }
//    |

//----
//- diagnostics
// Error: Missing `provider` field in client. e.g. `provider openai`
//    ╭─[ client_required_provider.baml:1:1 ]
//    │
//  1 │ ╭─▶ client<llm> MyClient {
//    ┆ ┆   
//  5 │ ├─▶ }
//    │ │       
//    │ ╰─────── Missing `provider` field in client. e.g. `provider openai`
//    │     
//    │     Note: Error code: E0026
// ───╯
