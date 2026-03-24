client<llm> InvalidFieldName {
  provider openai
  options {
    model "gpt-4"
    http {
      conect_timeout_ms 5000  // Typo in field name
    }
  }
}

// error: Unrecognized field 'conect_timeout_ms' in http configuration block. Did you mean 'connect_timeout_ms'?
//   -->  client/http_config_invalid_fields.baml:5
//    |
//  4 |     model "gpt-4"
//  5 |     http {
//  6 |       conect_timeout_ms 5000  // Typo in field name
//  7 |     }
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
