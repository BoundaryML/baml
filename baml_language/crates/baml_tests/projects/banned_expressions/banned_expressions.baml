// Chained assignment should be a compiler error.
function Foo() -> int {
  let a = let b = 42
  a
}

// Returning an assignment expression should be a compiler error.
function Bar() -> int {
  let b = 42
}

// ============================================================================
// Parenthesized assignment in expression position
// ============================================================================

// Parenthesized simple assignment: `(x = 5)` used as a value.
// The parser's binding-power guard prevents `a = b = c`, but parens
// bypass it.  AST lowering should reject assignment operators in
// expression context instead of silently defaulting to BinaryOp::Add.
function ParenAssignSimple() -> int {
  let x = 10;
  (x = 5)
}

// Parenthesized compound assignment used as a value.
function ParenCompoundAssign() -> int {
  let x = 10;
  (x += 5)
}

// Parenthesized assignment captured into a let binding.
function ParenAssignInLet() -> int {
  let x = 10;
  let y = (x = 5);
  y
}

// ============================================================================
// Parenthesized assignment through optional chains (null-handling LHS)
// ============================================================================

class Profile {
  name string
  address string?
  scores int[]?
  settings map<string, string>?
}

class User {
  id int
  profile Profile?
}

class Node {
  value int
  next Node?
  children Node[]?
  labels map<string, Node>?
}

// Parenthesized safe-assign on optional field chain used as a value.
// `(user?.profile?.name = "x")` is assignment in expression position.
function ParenSafeAssignField(user: User?) -> string? {
  let result = (user?.profile?.name = "new_name");
  result
}

// Parenthesized safe-assign on optional index used as a value.
function ParenSafeAssignIndex(items: int[]?) -> int? {
  let result = (items?.[0] = 42);
  result
}

// Parenthesized safe compound-assign through optional chain.
function ParenSafeCompoundAssign(user: User?) -> int? {
  let result = (user?.id += 1);
  result
}

// Parenthesized safe-assign as a return expression (tail position).
function ParenSafeAssignReturn(user: User?) -> string? {
  (user?.profile?.name = "updated")
}

// Parenthesized safe-assign on deep chain: field + index + field.
function ParenSafeAssignDeepChain(node: Node?) -> int? {
  let result = (node?.children?.[0].value = 99);
  result
}

// Parenthesized safe-assign on map index through optional chain.
function ParenSafeAssignMapIndex(node: Node?) -> int? {
  let result = (node?.labels?.["main"].value = 50);
  result
}
