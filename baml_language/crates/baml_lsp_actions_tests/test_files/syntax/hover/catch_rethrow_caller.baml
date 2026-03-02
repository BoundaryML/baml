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
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at hover_catch_rethrow_caller.baml:6:16
// function parent(req: int) -> bool
// // inferred throws: AuthenticationError
