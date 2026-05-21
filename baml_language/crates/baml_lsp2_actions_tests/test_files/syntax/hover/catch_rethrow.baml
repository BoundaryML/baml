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
// warning: unreachable arm
//     ╭─[ hover_catch_rethrow.baml:10:27 ]
//     │
//  10 │     AuthenticationError => { throw e }
//     │                           ──────┬─────  
//     │                                 ╰─────── unreachable arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
// warning: extraneous throws declaration: user.Errors
//     ╭─[ hover_catch_rethrow.baml:15:45 ]
//     │
//  15 │ function getProfile(req: int) -> bool throws Errors {
//     │                                             ───┬───  
//     │                                                ╰───── extraneous throws declaration: user.Errors
//     │ 
//     │ Note: Error code: E0097
// ────╯
//
//- on_hover expressions
// hover at hover_catch_rethrow.baml:6:16
// No hover content
