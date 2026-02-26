// Test stream expansion for literal types.
// Expected: literal fields get S=never, D=T, so simplify(never | T) = T.

class WithLiterals {
  type "resume"
  version 1
  enabled true
}
