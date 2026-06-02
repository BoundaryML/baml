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

//----
//- diagnostics
// error: type mismatch: expected string[] | null, got (string | null)[]
//     ╭─[ client_remap_role_non_string_allowed_roles.baml:1:1 ]
//     │
//   1 │ ╭─▶ client<llm> MyClient {
//     ┆ ┆   
//  10 │ ├─▶ }
//     │ │      
//     │ ╰────── type mismatch: expected string[] | null, got (string | null)[]
//     │     
//     │     Note: Error code: E0001
// ────╯
