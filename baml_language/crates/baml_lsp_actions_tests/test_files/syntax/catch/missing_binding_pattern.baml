function MayFail(x: int) -> string {
  "ok"
}

function MissingBindingPattern(x: int) -> string {
  MayFail(x) catch {
    let y = "hi";
    y
  }
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
// Error: Expected '(', found '{'
//    ╭─[ catch_missing_binding_pattern.baml:6:20 ]
//    │
//  6 │   MayFail(x) catch {
//    │                    ┬  
//    │                    ╰── Expected '(', found '{'
//    │ 
//    │ Note: Error code: E0010
// ───╯
