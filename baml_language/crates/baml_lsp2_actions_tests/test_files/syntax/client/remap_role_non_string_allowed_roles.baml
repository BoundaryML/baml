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
// E0001
//
//   × mismatched types
//     ╭─[client_remap_role_non_string_allowed_roles.baml:1:1]
//   1 │ ╭─▶ client<llm> MyClient {
//   2 │ │     provider openai
//   3 │ │     options {
//   4 │ │       model "gpt-4o"
//   5 │ │       allowed_roles ["user", 123, "assistant"]
//   6 │ │       remap_roles {
//   7 │ │         user "human"
//   8 │ │       }
//   9 │ │     }
//  10 │ ├─▶ }
//     · ╰──── expected `string`, found `123`
//     ╰────
