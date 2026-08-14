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
// E0097
//
//   ⚠ extraneous throws declaration: Errors
//     ╭─[hover_catch_rethrow.baml:15:53]
//  15 │ ╭─▶ function getProfile(req: int) -> bool throws Errors {
//  16 │ │     return true
//  17 │ ╰─▶ }
//     ╰────
//
//- on_hover expressions
// hover at hover_catch_rethrow.baml:6:16
// No hover content
