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
// Warning: Unreachable catch arm
//     ╭─[ throw_throws_never_unreachable_rethrow.baml:13:7 ]
//     │
//  13 │     Errors.AuthenticationError => { throw e }
//     │       ─────────────┬────────────  
//     │                    ╰────────────── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
