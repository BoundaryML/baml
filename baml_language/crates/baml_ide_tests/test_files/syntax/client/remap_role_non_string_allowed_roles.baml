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

//----
//- diagnostics
// Error: allowed_roles values must be quoted strings
//    ╭─[ client_remap_role_non_string_allowed_roles.baml:5:27 ]
//    │
//  5 │     allowed_roles ["user", 123, "assistant"]
//    │                           ──┬─  
//    │                             ╰─── allowed_roles values must be quoted strings
//    │ 
//    │ Note: Error code: E0044
// ───╯
// Error: Expected `string`, found `123`
//    ╭─[ client_remap_role_non_string_allowed_roles.baml:5:18 ]
//    │
//  5 │     allowed_roles ["user", 123, "assistant"]
//    │                  ─────────────┬─────────────  
//    │                               ╰─────────────── Expected `string`, found `123`
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: Expected `string`, found `123`
//    ╭─[ client_remap_role_non_string_allowed_roles.baml:5:18 ]
//    │
//  5 │     allowed_roles ["user", 123, "assistant"]
//    │                  ─────────────┬─────────────  
//    │                               ╰─────────────── Expected `string`, found `123`
//    │ 
//    │ Note: Error code: E0001
// ───╯
