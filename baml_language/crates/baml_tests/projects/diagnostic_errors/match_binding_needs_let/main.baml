// Repro: bare identifier in a guarded match arm is read as a TYPE pattern,
// not a binding. Without `let`, the user gets cryptic "unresolved type" /
// "unresolved name" errors. The fix adds an actionable hint pointing at the
// `let` binding form.
function grade(score: int) -> string {
  match (score) {
    n if n >= 90 => "A",
    _ => "F",
  }
}

// The correct form: `let n` binds the scrutinee to `n`.
function gradeOk(score: int) -> string {
  match (score) {
    let n if n >= 90 => "A",
    _ => "F",
  }
}
