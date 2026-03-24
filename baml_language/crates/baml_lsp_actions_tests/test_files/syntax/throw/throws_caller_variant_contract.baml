// Caller sees variant-precise contract from callee

enum Errors {
  AuthError
  NotFoundError
  InternalError
}

function MayFail(x: int) -> string throws Errors.AuthError | Errors.NotFoundError {
  match (x) {
    0 => throw Errors.AuthError,
    _ => throw Errors.NotFoundError
  }
}

function CallerGood(x: int) -> string {
  MayFail(x) catch (e) {
    Errors.AuthError => "auth",
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
// Error: extraneous throws declaration: user.AuthError, user.NotFoundError
//    ╭─[ throw_throws_caller_variant_contract.baml:9:42 ]
//    │
//  9 │ function MayFail(x: int) -> string throws Errors.AuthError | Errors.NotFoundError {
//    │                                          ────────────────────┬───────────────────  
//    │                                                              ╰───────────────────── extraneous throws declaration: user.AuthError, user.NotFoundError
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: throws contract violation: `user.AuthError | user.NotFoundError` is missing user.Errors.AuthError, user.Errors.NotFoundError
//    ╭─[ throw_throws_caller_variant_contract.baml:9:42 ]
//    │
//  9 │ function MayFail(x: int) -> string throws Errors.AuthError | Errors.NotFoundError {
//    │                                          ────────────────────┬───────────────────  
//    │                                                              ╰───────────────────── throws contract violation: `user.AuthError | user.NotFoundError` is missing user.Errors.AuthError, user.Errors.NotFoundError
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: unreachable arm
//     ╭─[ throw_throws_caller_variant_contract.baml:18:25 ]
//     │
//  18 │     Errors.AuthError => "auth",
//     │                         ───┬──  
//     │                            ╰──── unreachable arm
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: unreachable arm
//     ╭─[ throw_throws_caller_variant_contract.baml:19:29 ]
//     │
//  19 │     Errors.NotFoundError => "notfound"
//     │                             ─────┬────  
//     │                                  ╰────── unreachable arm
//     │ 
//     │ Note: Error code: E0001
// ────╯
