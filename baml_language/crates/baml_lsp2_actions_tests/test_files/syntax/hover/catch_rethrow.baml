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
//     ╭─[ hover_catch_rethrow.baml:10:28 ]
//     │
//  10 │     AuthenticationError => { throw e }
//     │                            ─────┬─────
//     │                                 ╰─────── unreachable arm
//     │
//     │ Note: Error code: E0063
// ────╯
// warning: extraneous throws declaration: AuthenticationError, AuthorizationError
//     ╭─[ hover_catch_rethrow.baml:15:46 ]
//     │
//  15 │ function getProfile(req: int) -> bool throws Errors {
//     │                                              ───┬──
//     │                                                 ╰──── extraneous throws declaration: AuthenticationError, AuthorizationError
//     │
//     │ Note: Error code: E0097
// ────╯
//
//- on_hover expressions
// hover at hover_catch_rethrow.baml:6:16
// No hover content
