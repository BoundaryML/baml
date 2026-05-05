// Phase 1 negative test: assigning a json value to an int variable should
// produce a TypeMismatch diagnostic.

function BadAssign(j: json) -> int {
  j
}
