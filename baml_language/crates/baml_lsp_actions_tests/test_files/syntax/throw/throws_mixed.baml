// throws mix of primitive + enum variant — no diagnostic when body matches

enum Errors {
  AuthError
  NotFoundError
}

function ThrowsMixed(x: int) -> string throws string | Errors.AuthError {
  match (x) {
    0 => throw "oops",
    _ => throw Errors.AuthError
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
// Error: extraneous throws declaration: user.AuthError
//    ╭─[ throw_throws_mixed.baml:8:48 ]
//    │
//  8 │ function ThrowsMixed(x: int) -> string throws string | Errors.AuthError {
//    │                                                ─────────────┬────────────  
//    │                                                             ╰────────────── extraneous throws declaration: user.AuthError
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: throws contract violation: `string | user.AuthError` is missing user.Errors.AuthError
//    ╭─[ throw_throws_mixed.baml:8:48 ]
//    │
//  8 │ function ThrowsMixed(x: int) -> string throws string | Errors.AuthError {
//    │                                                ─────────────┬────────────  
//    │                                                             ╰────────────── throws contract violation: `string | user.AuthError` is missing user.Errors.AuthError
//    │ 
//    │ Note: Error code: E0001
// ───╯
