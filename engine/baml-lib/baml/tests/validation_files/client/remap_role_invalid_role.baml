client<llm> MyClient {
  provider openai
  options {
    model "gpt-4o"
    allowed_roles ["user", "assistant"]
    remap_roles {
      user "human"
      system "instructions"
    }
  }
}

// error: remap_roles values must be one of: Value("user"), Value("assistant"). Got: system. To support different remap roles, add allowed_roles ["user", "assistant", "system", ...]
//   -->  client/remap_role_invalid_role.baml:8
//    | 
//  7 |       user "human"
//  8 |       system "instructions"
//    | 
