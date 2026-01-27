client<llm> MyClient {
  provider openai
  options {
    model "gpt-4o"
    remap_roles "not_a_map"
  }
}

// error: remap_roles must be a map. Got: string
//   -->  client/remap_role_invalid_type.baml:5
//    |
//  4 |     model "gpt-4o"
//  5 |     remap_roles "not_a_map"
//    |

//----
//- diagnostics
// Error: remap_roles must be a map. Got: string
//    ╭─[ client_remap_role_invalid_type.baml:5:16 ]
//    │
//  5 │     remap_roles "not_a_map"
//    │                ──────┬─────  
//    │                      ╰─────── remap_roles must be a map. Got: string
//    │ 
//    │ Note: Error code: E0044
// ───╯
