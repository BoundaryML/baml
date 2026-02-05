client<llm> MyClient {
  provider baml-openai-chat
  myExtraProp "hello"
  options {
    max_tokens 100
  }
}

// error: Error validating: Unknown field `myExtraProp` in client
//   -->  client/unknown_prop.baml:3
//    |
//  2 |   provider baml-openai-chat
//  3 |   myExtraProp "hello"
//    |

//----
//- diagnostics
// Error: Unknown field `myExtraProp` in client. Only `provider` and `options` are supported.
//    ╭─[ client_unknown_prop.baml:3:3 ]
//    │
//  3 │   myExtraProp "hello"
//    │   ─────┬─────  
//    │        ╰─────── Unknown field `myExtraProp` in client. Only `provider` and `options` are supported.
//    │ 
//    │ Note: Error code: E0027
// ───╯
