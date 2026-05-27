// Writes to outer bindings from inside a `let … else` else block must NOT
// be visible in the success continuation — the else branch diverges, so
// its effects are unreachable from the rest of the function.
//
// Pre-fix, `restore_scoped_locals` was used for the else snapshot, which
// is a JOIN-style merge designed for branches that return control (if /
// if-let). For let-else's diverging else we use the hard-rollback path
// `discard_scoped_locals` instead.
class Ok { value string }
class Err { message string }

function uses_outer(r: Ok | Err) -> int {
  let counter = 0;
  let o: Ok = r else {
    counter = 999;
    return -1;
  };
  // If the else's assignment leaked through, `counter` would appear as
  // assigned to 999 here; instead it's still 0.
  counter
}

//----
//- diagnostics
// <no-diagnostics-expected>
