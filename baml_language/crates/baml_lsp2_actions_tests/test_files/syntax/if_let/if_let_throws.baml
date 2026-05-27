// Throws analysis must flow through if-let: throws from the scrutinee and
// from either branch are part of the function's effective throws set.

// Scrutinee can throw — its throws must appear in the declared throws.
// Returns a union so the if-let pattern stays refutable.
function may_yield(x: int) -> int | string throws int {
  if (x < 0) {
    throw x
  } else {
    "fallback"
  }
}

// Positive: both `int` (from scrutinee call) and `string` (from then-branch
// throw) are declared. Refutable pattern keeps both branches live.
function ThrowsThroughIfLet(x: int) -> string throws int | string {
  if let n: int = may_yield(x) {
    throw "then-branch threw"
  } else {
    "no match"
  }
}

// Negative: declared throws missing the type thrown from the else-branch.
// Refutable scrutinee type so the else branch is reachable.
function ThrowsContractViolationInElse(v: int | string) -> string throws never {
  if let n: int = v {
    "matched"
  } else {
    throw "from else"
  }
}

//----
//- diagnostics
// error: throws contract violation: `never` is missing string
//     ╭─[ if_let_if_let_throws.baml:26:73 ]
//     │
//  26 │ function ThrowsContractViolationInElse(v: int | string) -> string throws never {
//     │                                                                         ───┬──  
//     │                                                                            ╰──── throws contract violation: `never` is missing string
//     │ 
//     │ Note: Error code: E0096
// ────╯
