class AuthenticationError {}
class AuthorizationError {}
class NotFoundError {}
class InternalServerError {}

type Errors = AuthenticationError | AuthorizationError | NotFoundError | InternalServerError

class Request {
  id string
  token string
}

class Profile {
  name string
  over18 bool
}

function server(req: Request) -> bool {
  {
    getProfile(req)
  } catch (e) {
    AuthenticationError => { throw e }
    _ => { false }
  }
}

function getProfile(req: Request) -> bool throws Errors {
  return true
}

//----
//- diagnostics
// E0097
//
//   ⚠ extraneous throws declaration: Errors
//     ╭─[throw_throws_type_alias_repro.baml:27:57]
//  27 │ ╭─▶ function getProfile(req: Request) -> bool throws Errors {
//  28 │ │     return true
//  29 │ ╰─▶ }
//     ╰────
