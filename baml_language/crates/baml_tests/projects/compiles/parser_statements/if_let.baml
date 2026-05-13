// Tests for `if let` expressions, modeled on Rust's `if let`.
//
// `if let <pat> = <expr> { then } [else { else }]` desugars at AST lowering
// time to a `match` expression with two arms:
//   match (<expr>) { <pat> => <then>, _ => <else_or_unit> }

function if_let_type_narrow_int() -> int {
  let x: int | string = 7;
  if let y: int = x {
    return y + 1;
  }
  return -1;
}

function if_let_type_narrow_string() -> string {
  let x: int | string = "hi";
  if let y: string = x {
    return y;
  }
  return "fallback";
}

function if_let_with_else() -> int {
  let x: int | string = "nope";
  if let y: int = x {
    return y;
  } else {
    return -1;
  }
}

function if_let_with_else_value() -> int {
  let x: int | string = 5;
  let result = if let y: int = x {
    y * 2
  } else {
    0
  };
  return result;
}

function if_let_else_if() -> int {
  let x: int | string | bool = true;
  if let y: int = x {
    return y;
  } else if let z: string = x {
    return 0;
  } else {
    return -1;
  }
}

function if_let_else_if_let_chain() -> int {
  let x: int | string = "abc";
  if let a: int = x {
    return a;
  } else if let b: string = x {
    return 1;
  } else {
    return -2;
  }
}

function if_let_nested() -> int {
  let outer: int | string = 3;
  let inner: int | string = 4;
  if let a: int = outer {
    if let b: int = inner {
      return a + b;
    } else {
      return a;
    }
  }
  return -1;
}

// NOTE: irrefutable `if let` (e.g. `if let y = 42 { ... }`) is intentionally
// not covered here — BAML's match exhaustiveness check reports the
// fall-through arm as "unreachable arm", which keeps the program out of the
// `compiles/` tier. Rust rejects the same pattern at the parser level.

function if_let_as_tail_expr() -> int {
  let v: int | string = 10;
  // Block tail expression — no trailing semicolon.
  if let n: int = v {
    n
  } else {
    -1
  }
}
