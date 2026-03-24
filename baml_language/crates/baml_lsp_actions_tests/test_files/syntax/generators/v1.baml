generator lang_python_1 {
  output_type "python/pydantic"
  output_dir "../"
}

generator lang_python_2 {
  output_type "python/pydantic"
  default_client_mode "async"
  output_dir "../"
}

generator lang_python_3 {
  output_type "python/pydantic"
  default_client_mode "sync"
  output_dir "../"
}

generator lang_typescript_1 {
  output_type typescript
  default_client_mode "async"
  output_dir "../"
}


generator lang_typescript_2 {
  output_type typescript
  default_client_mode "sync"
  output_dir "../"
}

generator lang_ruby_1 {
  output_type typescript
  default_client_mode "async"
  output_dir "../"
}

generator lang_ruby_2 {
  output_type typescript
  default_client_mode "sync"
  output_dir "../"
}

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
