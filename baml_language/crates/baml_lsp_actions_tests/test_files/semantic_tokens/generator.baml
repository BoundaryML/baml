generator lang_python {
  output_type "python/pydantic"
  output_dir "../python"
  version "0.68.0"
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
//
//- semantic_tokens
// generator.baml:1:1 (keyword) len=9 "generator"
// generator.baml:1:11 (struct) len=11 "lang_python"
// generator.baml:2:3 (property) len=11 "output_type"
// generator.baml:2:15 (string) len=1 "\""
// generator.baml:2:16 (string) len=6 "python"
// generator.baml:2:22 (string) len=1 "/"
// generator.baml:2:23 (string) len=8 "pydantic"
// generator.baml:2:31 (string) len=1 "\""
// generator.baml:3:3 (property) len=10 "output_dir"
// generator.baml:3:14 (string) len=1 "\""
// generator.baml:3:15 (string) len=1 "."
// generator.baml:3:16 (string) len=1 "."
// generator.baml:3:17 (string) len=1 "/"
// generator.baml:3:18 (string) len=6 "python"
// generator.baml:3:24 (string) len=1 "\""
// generator.baml:4:3 (property) len=7 "version"
// generator.baml:4:11 (string) len=1 "\""
// generator.baml:4:12 (string) len=4 "0.68"
// generator.baml:4:16 (string) len=1 "."
// generator.baml:4:17 (string) len=1 "0"
// generator.baml:4:18 (string) len=1 "\""
