// Throw as expression (not just statement) — used in expression position
function MayFail(x: int) -> string {
  "ok"
}

// Throw in if-else expression (both branches)
function ThrowInIfElse(x: int) -> string {
  if (x > 0) {
    "positive"
  } else {
    throw "negative not allowed"
  }
}

// Throw in match arm
function ThrowInMatchArm(x: int) -> string {
  match (x) {
    0 => throw "zero not allowed",
    _ => "ok"
  }
}

// Throw in catch arm (expression form)
function ThrowExpressionInCatch(x: int) -> string {
  MayFail(x) catch (e) {
    _ => throw "re-throw as expression"
  }
}

//----
//- diagnostics
// Warning: Unreachable catch arm
//     ╭─[ throw_throw_expression.baml:26:7 ]
//     │
//  26 │     _ => throw "re-throw as expression"
//     │       ┬  
//     │       ╰── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
