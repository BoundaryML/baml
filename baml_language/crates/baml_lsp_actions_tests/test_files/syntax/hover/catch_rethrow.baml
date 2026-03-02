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
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at hover_catch_rethrow.baml:6:16
// function server(req: int) -> bool
// // inferred throws: AuthenticationError
