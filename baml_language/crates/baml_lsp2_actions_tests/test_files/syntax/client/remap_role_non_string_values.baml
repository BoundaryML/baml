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

//----
//- diagnostics
// E0001
//
//   × mismatched types
//     ╭─[client_remap_role_non_string_values.baml:1:1]
//   1 │ ╭─▶ client<llm> MyClient {
//   2 │ │     provider openai
//   3 │ │     options {
//   4 │ │       model "gpt-4o"
//   5 │ │       remap_roles {
//   6 │ │         user "human"
//   7 │ │         assistant 123
//   8 │ │       }
//   9 │ │     }
//  10 │ ├─▶ }
//     · ╰──── expected `string`, found `123`
//     ╰────
