// Scope-bleed regressions for `if let`, `while let`, and `let else`.
//
// Each function below references a binding outside the scope where it was
// introduced. The expected diagnostic is `unresolved name` for that binding.

// ── `if let` ───────────────────────────────────────────────────────────────

// The `if let` binding is NOT visible after the construct.
function if_let_binding_not_visible_after(v: int | string) -> int {
  if let x: int = v {
    return x;
  }
  // `x` is out of scope here.
  return x;
}

// The `if let` binding is NOT visible in its else branch.
function if_let_binding_not_visible_in_else(v: int | string) -> int {
  if let x: int = v {
    return x;
  } else {
    // `x` is not in scope here — it only exists in the then-branch.
    return x;
  }
}

// The `if let` binding from one arm is NOT visible to a sibling `else if let`.
function if_let_binding_not_visible_to_sibling(v: int | string) -> int {
  if let x: int = v {
    return x;
  } else if let y: string = v {
    // `x` is out of scope; only `y` is bound here.
    return x;
  } else {
    return -1;
  }
}

// ── `while let` ────────────────────────────────────────────────────────────

// The `while let` binding is NOT visible after the loop terminates.
function while_let_binding_not_visible_after(v: int | string) -> int {
  while let n: int = v {
    v = "stop";
  };
  // `n` is out of scope here.
  return n;
}

// ── `let ... else` ─────────────────────────────────────────────────────────

// The let-else binding is NOT visible inside the diverging else block.
function let_else_binding_not_visible_in_else(raw: int | string) -> int {
  let x: int = raw else {
    // `x` is not yet in scope — bindings come into scope AFTER the else block.
    return x;
  };
  return x;
}

// Same idea but the else block tries to use the binding to make a decision.
function let_else_uses_binding_in_else_predicate(raw: int | string) -> int {
  let n: int = raw else {
    if (n > 0) {
      return 1;
    }
    return -1;
  };
  return n;
}
