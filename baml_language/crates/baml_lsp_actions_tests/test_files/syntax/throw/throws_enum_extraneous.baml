// throws whole enum, body only throws one variant — E0097 for unused variants

enum Errors {
  AuthError
  NotFoundError
  InternalError
}

function ThrowsEnumExtraneous(x: int) -> string throws Errors {
  throw Errors.AuthError
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
// Error: extraneous throws declaration: user.Errors
//    ╭─[ throw_throws_enum_extraneous.baml:9:57 ]
//    │
//  9 │ function ThrowsEnumExtraneous(x: int) -> string throws Errors {
//    │                                                         ───┬───  
//    │                                                            ╰───── extraneous throws declaration: user.Errors
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: throws contract violation: `user.Errors` is missing user.Errors.AuthError
//    ╭─[ throw_throws_enum_extraneous.baml:9:57 ]
//    │
//  9 │ function ThrowsEnumExtraneous(x: int) -> string throws Errors {
//    │                                                         ───┬───  
//    │                                                            ╰───── throws contract violation: `user.Errors` is missing user.Errors.AuthError
//    │ 
//    │ Note: Error code: E0001
// ───╯
