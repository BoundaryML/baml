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
// warning: unreachable arm
//     ╭─[ throw_throws_never_unreachable_rethrow.baml:13:34 ]
//     │
//  13 │     Errors.AuthenticationError => { throw e }
//     │                                  ──────┬─────  
//     │                                        ╰─────── unreachable arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
// error: throws contract violation: `never` is missing user.Errors.AuthenticationError
//     ╭─[ throw_throws_never_unreachable_rethrow.baml:11:41 ]
//     │
//  11 │ function server(req: int) -> bool throws never {
//     │                                         ───┬──  
//     │                                            ╰──── throws contract violation: `never` is missing user.Errors.AuthenticationError
//     │ 
//     │ Note: Error code: E0096
// ────╯
