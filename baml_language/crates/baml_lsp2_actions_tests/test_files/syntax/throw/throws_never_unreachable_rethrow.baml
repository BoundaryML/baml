// throws never with catch on non-throwing callee — rethrow in unreachable arm is dead code

enum Errors {
  AuthenticationError
}

function getProfile(req: int) -> bool {
  return true
}

function server(req: int) -> bool throws never {
  getProfile(req) catch (e) {
    Errors.AuthenticationError => { throw e }
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
