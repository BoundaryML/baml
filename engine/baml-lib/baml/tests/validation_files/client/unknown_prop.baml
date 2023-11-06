client<llm> MyClient {
  provider baml-openai-chat
  myExtraProp "hello"
  options {
    max_tokens 100
  }
}

// error: Error validating: Unknown field `myExtraProp` in client
//   -->  /Users/aaronvillalpando/Projects/gloo-lang/engine/baml/baml/tests/validation_files/client/unknown_prop.baml:3
//    | 
//  2 |   provider baml-openai-chat
//  3 |   myExtraProp "hello"
//    | 
