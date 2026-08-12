// Test: semantic tokens for multi-segment Path expressions.
// Issue #9: The final segment should get a type-aware token kind,
// not a generic "property" token for everything after the root.

enum Status {
  Active
  Inactive
}

class User {
  name string

  function greet(self) -> string {
    self.name
  }
}

function test_tokens() -> string {
  // Status.Active — "Active" should be enumMember, not property
  let s = Status.Active

  // user.greet() — "greet" should be function, not property
  let u = User { name: "hi" }
  let g = u.greet()

  // user.name — "name" should be property (this is correct already)
  u.name
}
