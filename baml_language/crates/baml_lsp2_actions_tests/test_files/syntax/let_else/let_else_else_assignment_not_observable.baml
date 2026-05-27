// Writes to outer bindings from inside a `let … else` else block must NOT
// be visible in the success continuation — the else branch diverges, so
// its effects are unreachable from the rest of the function.
//
// Pre-fix, `restore_scoped_locals` was used for the else snapshot, which
// is a JOIN-style merge designed for branches that return control (if /
// if-let). For let-else's diverging else we use the hard-rollback path
// `discard_scoped_locals` instead.
//
// Tested here at the integration level rather than as a Rust unit test
// against `discard_scoped_locals` directly: the helper itself is a
// three-line truncation whose only meaningful behavior is its
// interaction with the surrounding TIR check pass. A standalone unit
// test would have to reconstruct the TIR builder's state by hand and
// would end up restating the implementation; the .baml form below
// drives the actual code path the way real programs do.
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
