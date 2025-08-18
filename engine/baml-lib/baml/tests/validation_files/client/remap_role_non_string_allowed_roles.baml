client<llm> MyClient {
  provider openai
  options {
    model "gpt-4o"
    allowed_roles ["user", 123, "assistant"]
    remap_roles {
      user "human"
    }
  }
}

// error: values in allowed_roles must be strings. Got: number
//   -->  client/remap_role_non_string_allowed_roles.baml:5
//    | 
//  4 |     model "gpt-4o"
//  5 |     allowed_roles ["user", 123, "assistant"]
//    | 
