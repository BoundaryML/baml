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
// E0097
//
//   ⚠ extraneous throws declaration: Errors
//     ╭─[hover_catch_rethrow_caller.baml:23:53]
//  23 │ ╭─▶ function getProfile(req: int) -> bool throws Errors {
//  24 │ │     return true
//  25 │ ╰─▶ }
//     ╰────
//
//- on_hover expressions
// hover at hover_catch_rethrow_caller.baml:6:16
// No hover content
