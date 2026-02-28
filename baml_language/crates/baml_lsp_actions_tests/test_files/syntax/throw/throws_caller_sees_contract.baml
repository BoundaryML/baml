// Caller sees declared throws contract, not body-derived facts

enum Errors {
  AuthError
  NotFoundError
  InternalError
}

function MayFail(x: int) -> string throws Errors {
  throw Errors.AuthError
}

function Caller(x: int) -> string {
  MayFail(x) catch (e) {
    Errors.AuthError => "auth",
    Errors.NotFoundError => "notfound",
    Errors.InternalError => "internal"
  }
}

//----
//- diagnostics
// Warning: `throws` declaration includes types the function never throws: Errors.InternalError, Errors.NotFoundError
//    ╭─[ throw_throws_caller_sees_contract.baml:9:28 ]
//    │
//  9 │ function MayFail(x: int) -> string throws Errors {
//    │                            ───┬───  
//    │                               ╰───── `throws` declaration includes types the function never throws: Errors.InternalError, Errors.NotFoundError
//    │ 
//    │ Note: Error code: E0097
// ───╯
