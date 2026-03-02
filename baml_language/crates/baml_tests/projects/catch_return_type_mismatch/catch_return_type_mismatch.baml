// Regression: return inside catch arm must be validated against function return type.

enum Errors {
  AuthenticationError
  AuthorizationError
  NotFoundError
  InternalServerError
}

function callee() -> bool throws Errors {
  return true
}

// BUG: `return e` returns an Errors enum but function declares `-> bool`.
// The type checker must report a mismatch.
function caller() -> bool {
  callee() catch (e) {
    Errors.AuthenticationError => { return e }
  }
}
