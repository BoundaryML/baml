client<llm> MyClient {
  provider baml-openai-chat
  options {
    model gpt-3.5-turbo
    max_tokens 100
  }
}

// error: Error validating Client "MyClient": This field declaration is invalid. It is either missing a name or a type.
//   -->  client/period_in_model_type.baml:3
//    | 
//  2 |   provider baml-openai-chat
//  3 |   options {
//    | 
// error: Error validating: This line is not a valid field or attribute definition. A valid property may look like: 'myProperty "some value"' for example, with no colons.
//   -->  client/period_in_model_type.baml:3
//    | 
//  2 |   provider baml-openai-chat
//  3 |   options {
//  4 |     model gpt-3.5-turbo
//    | 
// error: Error validating: This line is invalid. It does not start with any known Baml schema keyword.
//   -->  client/period_in_model_type.baml:7
//    | 
//  6 |   }
//  7 | }
//    | 
// error: Error validating: Unknown field `model` in client
//   -->  client/period_in_model_type.baml:4
//    | 
//  3 |   options {
//  4 |     model gpt-3.5-turbo
//    | 
// error: Error validating: Unknown field `max_tokens` in client
//   -->  client/period_in_model_type.baml:5
//    | 
//  4 |     model gpt-3.5-turbo
//  5 |     max_tokens 100
//    | 
