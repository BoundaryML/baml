// Parser-level error cases for `if let`, `while let`, and `let else`.
//
// Each function below uses one of these constructs malformed. The parser
// must recover and continue parsing the next item, even when the head of
// a construct is unrecognisable.

// `if let` with no pattern.
function if_let_missing_pattern(v: int | string) -> int {
  if let = v {
    return 1;
  }
  return 0;
}

// `if let` with no `=` separator (no initializer at all).
function if_let_missing_equals(v: int | string) -> int {
  if let x {
    return 1;
  }
  return 0;
}

// `if let` with no initializer expression.
function if_let_missing_initializer(v: int | string) -> int {
  if let x: int = {
    return 1;
  }
  return 0;
}

// `if let` with no body block.
function if_let_missing_body(v: int | string) -> int {
  if let x: int = v
}

// `while let` with no body block.
function while_let_missing_body(v: int | string) -> int {
  while let n: int = v;
  return 0;
}

// `while let` with no pattern.
function while_let_missing_pattern(v: int | string) -> int {
  while let = v {
  };
  return 0;
}

// let-else missing the else block.
function let_else_missing_else_block(raw: int | string) -> int {
  let x: int = raw else;
  return x;
}

// let-else with a non-block else body (expression instead of `{ ... }`).
function let_else_non_block_else(raw: int | string) -> int {
  let x: int = raw else 0;
  return x;
}

// let-else missing the trailing semicolon.
function let_else_missing_semicolon(raw: int | string) -> int {
  let x: int = raw else { return -1; }
  return x;
}

// `else if let` with no `=`.
function else_if_let_missing_equals(v: int | string) -> int {
  if (false) {
    return 1;
  } else if let x {
    return 2;
  }
  return 0;
}

// Recovery anchor: this function MUST still parse after the broken ones
// above, confirming the parser hasn't latched onto an infinite-loop state.
function recovery_anchor() -> int {
  return 42;
}
