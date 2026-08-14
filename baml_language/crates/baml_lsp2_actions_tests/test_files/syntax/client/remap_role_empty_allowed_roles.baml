client<llm> MyClient {
  provider openai
  options {
    model "gpt-4o"
    allowed_roles []
    remap_roles {
      user "human"
    }
  }
}

// error: allowed_roles must not be empty
//   -->  client/remap_role_empty_allowed_roles.baml:5
//    |
//  4 |     model "gpt-4o"
//  5 |     allowed_roles []
//    |
// error: remap_roles values must be one of: . Got: user. To support different remap roles, add allowed_roles ["user", "assistant", "system", ...]
//   -->  client/remap_role_empty_allowed_roles.baml:7
//    |
//  6 |     remap_roles {
//  7 |       user "human"
//    |

//----
//- diagnostics
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client MyClient = openai.OpenAiClient.new(model = "...");` (compose reliability with ai.Retry / ai.Fallback /
//   │ ai.RoundRobin)
//     ╭─[client_remap_role_empty_allowed_roles.baml:1:1]
//   1 │ ╭─▶ client<llm> MyClient {
//   2 │ │     provider openai
//   3 │ │     options {
//   4 │ │       model "gpt-4o"
//   5 │ │       allowed_roles []
//   6 │ │       remap_roles {
//   7 │ │         user "human"
//   8 │ │       }
//   9 │ │     }
//  10 │ ├─▶ }
//     · ╰──── replace with `client Name = <expr>;`
//     ╰────
