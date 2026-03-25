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
// Error: unreachable arm
//     ╭─[ throw_throws_type_alias_repro.baml:22:34 ]
//     │
//  22 │     Errors.AuthenticationError => { throw e }
//     │                                  ──────┬─────  
//     │                                        ╰─────── unreachable arm
//     │ 
//     │ Note: Error code: E0001
// ────╯
// Error: extraneous throws declaration: user.Errors
//     ╭─[ throw_throws_type_alias_repro.baml:27:49 ]
//     │
//  27 │ function getProfile(req: Request) -> bool throws Errors {
//     │                                                 ───┬───  
//     │                                                    ╰───── extraneous throws declaration: user.Errors
//     │ 
//     │ Note: Error code: E0001
// ────╯
