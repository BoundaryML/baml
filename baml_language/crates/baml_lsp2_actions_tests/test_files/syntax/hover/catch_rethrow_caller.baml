class AuthenticationError {}
class AuthorizationError {}

type Errors = AuthenticationError | AuthorizationError

function parent<[CURSOR](req: int) -> bool {
  server(req)

  return true
}

function server(req: int) -> bool {
  {
    getProfile(req)
  } catch (e) {
    AuthenticationError => { throw e }
    _ => { false }
  }

  return true
}

function getProfile(req: int) -> bool throws Errors {
  return true
}

//----
//- diagnostics
// warning: unreachable arm
//     ╭─[ hover_catch_rethrow_caller.baml:16:27 ]
//     │
//  16 │     AuthenticationError => { throw e }
//     │                           ──────┬─────  
//     │                                 ╰─────── unreachable arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
// warning: extraneous throws declaration: user.Errors
//     ╭─[ hover_catch_rethrow_caller.baml:23:45 ]
//     │
//  23 │ function getProfile(req: int) -> bool throws Errors {
//     │                                             ───┬───  
//     │                                                ╰───── extraneous throws declaration: user.Errors
//     │ 
//     │ Note: Error code: E0097
// ────╯
//
//- on_hover expressions
// hover at hover_catch_rethrow_caller.baml:6:16
// No hover content
