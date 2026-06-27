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
//     ╭─[ throw_throws_never_unreachable_rethrow.baml:13:35 ]
//     │
//  13 │     Errors.AuthenticationError => { throw e }
//     │                                   ─────┬─────
//     │                                        ╰─────── unreachable arm
//     │
//     │ Note: Error code: E0063
// ────╯
// error: declared throws is `never`, but this function may also throw `Errors.AuthenticationError`
//     ╭─[ throw_throws_never_unreachable_rethrow.baml:11:42 ]
//     │
//  11 │ function server(req: int) -> bool throws never {
//     │                                          ──┬──
//     │                                            ╰──── declared throws is `never`, but this function may also throw `Errors.AuthenticationError`
//     │
//     │ Note: Error code: E0096
// ────╯
