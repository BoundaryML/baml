// User-throw fixture for the error-return contract (31b-phase1). A
// user-declared error class thrown from a function whose `throws` clause
// names it. The host call is unhandled, so it must surface in Python as a
// `BamlError` whose `.value` is the decoded `MyError` instance (proving a
// user type round-trips structured, not stringified).

class MyError {
  code int
  detail string
}

function ThrowMyError() -> int throws MyError {
  throw MyError { code: 42, detail: "boom" }
}
