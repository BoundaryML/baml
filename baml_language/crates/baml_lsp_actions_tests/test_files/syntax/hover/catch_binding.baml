enum Errors {
  AuthError
  NotFoundError
}

function MayFail(x: int) -> string {
  match (x) {
    0 => throw Errors.AuthError,
    1 => throw Errors.NotFoundError,
    _ => "ok"
  }
}

function Handler(x: int) -> string {
  MayFail(x) catch (<[CURSOR]e) {
    Errors.AuthError => { throw e }
    Errors.NotFoundError => "notfound"
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
//
//- on_hover expressions
// hover at hover_catch_binding.baml:15:21
// No hover content
