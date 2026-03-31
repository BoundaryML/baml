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
// Error: unreachable arm
//     ╭─[ throw_throws_never_unreachable_rethrow.baml:13:36 ]
//     │
//  13 │ ╭─▶     Errors.AuthenticationError => { throw e }
//  14 │ ├─▶   }
//     │ │         
//     │ ╰───────── unreachable arm
//     │     
//     │     Note: Error code: E0001
// ────╯
// Error: throws contract violation: `never` is missing baml.panics.AssertionFailed, baml.panics.DivisionByZero, baml.panics.IndexOutOfBounds, baml.panics.KeyNotFound, baml.panics.NegativeIndex, baml.panics.StackOverflow, baml.panics.Unreachable
//     ╭─[ throw_throws_never_unreachable_rethrow.baml:11:43 ]
//     │
//  11 │ function server(req: int) -> bool throws never {
//     │                                           ───┬──  
//     │                                              ╰──── throws contract violation: `never` is missing baml.panics.AssertionFailed, baml.panics.DivisionByZero, baml.panics.IndexOutOfBounds, baml.panics.KeyNotFound, baml.panics.NegativeIndex, baml.panics.StackOverflow, baml.panics.Unreachable
//     │ 
//     │ Note: Error code: E0001
// ────╯
