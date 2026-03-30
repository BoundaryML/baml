client<llm> MyClient {
  provider openai
  myExtraProp "hello"
  options {
    max_tokens 100
  }
}

// error: Error validating: Unknown field `myExtraProp` in client
//   -->  client/unknown_prop.baml:3
//    |
//  2 |   provider openai
//  3 |   myExtraProp "hello"
//    |

//----
//- diagnostics
// <no-diagnostics-expected>
