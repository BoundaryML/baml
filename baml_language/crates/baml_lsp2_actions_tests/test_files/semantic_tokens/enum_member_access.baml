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

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// enum_member_access.baml:1:1 (comment) len=60 "// Test: semantic tokens for multi-segment Path expressions."
// enum_member_access.baml:2:1 (comment) len=66 "// Issue #9: The final segment should get a type-aware token kind,"
// enum_member_access.baml:3:1 (comment) len=64 "// not a generic \"property\" token for everything after the root."
// enum_member_access.baml:5:1 (keyword) len=4 "enum"
// enum_member_access.baml:5:6 (enum) [declaration] len=6 "Status"
// enum_member_access.baml:6:3 (enumMember) [declaration] len=6 "Active"
// enum_member_access.baml:7:3 (enumMember) [declaration] len=8 "Inactive"
// enum_member_access.baml:10:1 (keyword) len=5 "class"
// enum_member_access.baml:10:7 (class) [declaration] len=4 "User"
// enum_member_access.baml:11:3 (property) [declaration] len=4 "name"
// enum_member_access.baml:11:8 (type) [defaultLibrary] len=6 "string"
// enum_member_access.baml:13:3 (keyword) len=8 "function"
// enum_member_access.baml:13:12 (method) [declaration] len=5 "greet"
// enum_member_access.baml:13:18 (parameter) [declaration] len=4 "self"
// enum_member_access.baml:13:27 (type) [defaultLibrary] len=6 "string"
// enum_member_access.baml:14:5 (parameter) len=4 "self"
// enum_member_access.baml:14:10 (property) len=4 "name"
// enum_member_access.baml:18:1 (keyword) len=8 "function"
// enum_member_access.baml:18:10 (function) [declaration] len=11 "test_tokens"
// enum_member_access.baml:18:27 (type) [defaultLibrary] len=6 "string"
// enum_member_access.baml:19:3 (comment) len=64 "// Status.Active — \"Active\" should be enumMember, not property"
// enum_member_access.baml:20:3 (keyword) len=3 "let"
// enum_member_access.baml:20:7 (variable) [declaration] len=1 "s"
// enum_member_access.baml:20:9 (operator) len=1 "="
// enum_member_access.baml:20:11 (enum) len=6 "Status"
// enum_member_access.baml:20:18 (enumMember) len=6 "Active"
// enum_member_access.baml:22:3 (comment) len=60 "// user.greet() — \"greet\" should be function, not property"
// enum_member_access.baml:23:3 (keyword) len=3 "let"
// enum_member_access.baml:23:7 (variable) [declaration] len=1 "u"
// enum_member_access.baml:23:9 (operator) len=1 "="
// enum_member_access.baml:23:11 (class) len=4 "User"
// enum_member_access.baml:23:18 (property) len=4 "name"
// enum_member_access.baml:23:24 (string) len=4 "\"hi\""
// enum_member_access.baml:24:3 (keyword) len=3 "let"
// enum_member_access.baml:24:7 (variable) [declaration] len=1 "g"
// enum_member_access.baml:24:9 (operator) len=1 "="
// enum_member_access.baml:24:11 (variable) len=1 "u"
// enum_member_access.baml:24:13 (method) len=5 "greet"
// enum_member_access.baml:26:3 (comment) len=68 "// user.name — \"name\" should be property (this is correct already)"
// enum_member_access.baml:27:3 (variable) len=1 "u"
// enum_member_access.baml:27:5 (property) len=4 "name"
