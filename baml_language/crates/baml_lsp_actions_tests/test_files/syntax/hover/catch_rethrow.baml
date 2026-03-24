class AuthenticationError {}
class AuthorizationError {}

type Errors = AuthenticationError | AuthorizationError

function server<[CURSOR](req: int) -> bool {
  {
    getProfile(req)
  } catch (e) {
    AuthenticationError => { throw e }
    _ => { false }
  }
}

function getProfile(req: int) -> bool throws Errors {
  return true
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
// Error: unreachable arm
//     ╭─[ hover_catch_rethrow.baml:10:27 ]
//     │
//  10 │     AuthenticationError => { throw e }
//     │                           ──────┬─────  
//     │                                 ╰─────── unreachable arm
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: extraneous throws declaration: user.Errors
//     ╭─[ hover_catch_rethrow.baml:15:45 ]
//     │
//  15 │ function getProfile(req: int) -> bool throws Errors {
//     │                                             ───┬───  
//     │                                                ╰───── extraneous throws declaration: user.Errors
//     │ 
//     │ Note: Error code: E0001
// ────╯
//
//- on_hover expressions
// hover at hover_catch_rethrow.baml:6:16
// No hover content
