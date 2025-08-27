client<llm> MyClient {
  provider openai
  options {
    model "gpt-4o"
    remap_roles {
      user "human"
      assistant 123
    }
  }
}

// error: remap_role must be a map of strings to strings. Got: number
//   -->  client/remap_role_non_string_values.baml:7
//    | 
//  6 |       user "human"
//  7 |       assistant 123
//    | 
