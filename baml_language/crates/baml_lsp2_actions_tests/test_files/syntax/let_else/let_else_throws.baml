// Throws-analysis must flow through let-else: throws from the initializer
// and from the else branch escape into the enclosing function's throws set.

function may_yield(x: int) -> int | string throws int {
  if (x < 0) { throw x } else { "fallback" }
}

class NoMatch {}

// Positive: declared throws covers `int` (from initializer) and `NoMatch`
// (thrown from the else branch).
function ThrowsThroughLetElse(x: int) -> int throws int | NoMatch {
  let n: int = may_yield(x) else { throw NoMatch {}; };
  n
}

// Negative: declared throws missing the type thrown from the else branch.
function ThrowsContractViolationInElse(v: int | string) -> int throws never {
  let n: int = v else { throw "from else"; };
  n
}

//----
//- diagnostics
// E0096
//
//   × declared throws is `never`, but this function may also throw `string`
//     ╭─[let_else_let_else_throws.baml:19:31]
//  19 │   let n: int = v else { throw "from else"; };
//     ·                               ───────────
//     ╰────
