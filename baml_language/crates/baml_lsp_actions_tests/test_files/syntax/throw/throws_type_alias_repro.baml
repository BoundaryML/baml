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
    Errors.AuthenticationError => { throw e }
    _ => { false }
  }
}

function getProfile(req: Request) -> bool throws Errors {
  return true
}

//----
//- diagnostics
// Warning: Unreachable catch arm
//     ╭─[ throw_throws_type_alias_repro.baml:22:5 ]
//     │
//  22 │     Errors.AuthenticationError => { throw e }
//     │     ─────────────┬────────────  
//     │                  ╰────────────── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
