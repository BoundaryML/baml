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
// error: type mismatch: expected map<string, string>?, got "not_a_map"
//    ╭─[ client_remap_role_invalid_type.baml:1:1 ]
//    │
//  1 │ ╭─▶ client<llm> MyClient {
//    ┆ ┆
//  7 │ ├─▶ }
//    │ │
//    │ ╰─────── type mismatch: expected map<string, string>?, got "not_a_map"
//    │
//    │     Note: Error code: E0001
// ───╯
