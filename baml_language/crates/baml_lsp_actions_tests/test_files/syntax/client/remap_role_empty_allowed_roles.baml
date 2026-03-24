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
// Error: unreachable arm
//      ╭─[ llm_types.baml:159:17 ]
//      │
//  159 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:189:17 ]
//      │
//  189 │ ╭─▶             _ => {
//      ┆ ┆   
//  194 │ ├─▶             }
//      │ │                   
//      │ ╰─────────────────── unreachable arm
//      │     
//      │     Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:208:17 ]
//      │
//  208 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
