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
// warning: unreachable arm
//     ╭─[ throw_throws_type_alias_repro.baml:22:27 ]
//     │
//  22 │     AuthenticationError => { throw e }
//     │                           ──────┬─────  
//     │                                 ╰─────── unreachable arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
// warning: extraneous throws declaration: user.Errors
//     ╭─[ throw_throws_type_alias_repro.baml:27:49 ]
//     │
//  27 │ function getProfile(req: Request) -> bool throws Errors {
//     │                                                 ───┬───  
//     │                                                    ╰───── extraneous throws declaration: user.Errors
//     │ 
//     │ Note: Error code: E0097
// ────╯
