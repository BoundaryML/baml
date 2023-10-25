client<llm> MyClient {
  options {
    max_tokens 100
  }
}

// error: The required property "provider" on "MyClient" is missing.
//   -->  /Users/aaronvillalpando/Projects/gloo-lang/engine/baml/baml/tests/validation_files/client/required_provider.baml:1
//    | 
//    | 
//  1 | client<llm> MyClient {
//  2 |   options {
//  3 |     max_tokens 100
//  4 |   }
//  5 | }
//    | 
