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
// multi_segment_path.baml:1:1 (comment) len=60 "// Test: semantic tokens for multi-segment Path expressions."
// multi_segment_path.baml:2:1 (comment) len=66 "// Issue #9: The final segment should get a type-aware token kind,"
// multi_segment_path.baml:3:1 (comment) len=64 "// not a generic \"property\" token for everything after the root."
// multi_segment_path.baml:5:1 (keyword) len=4 "enum"
// multi_segment_path.baml:5:6 (enum) [declaration] len=6 "Status"
// multi_segment_path.baml:6:3 (enumMember) [declaration] len=6 "Active"
// multi_segment_path.baml:7:3 (enumMember) [declaration] len=8 "Inactive"
// multi_segment_path.baml:10:1 (keyword) len=5 "class"
// multi_segment_path.baml:10:7 (class) [declaration] len=4 "User"
// multi_segment_path.baml:11:3 (property) [declaration] len=4 "name"
// multi_segment_path.baml:11:8 (type) [defaultLibrary] len=6 "string"
// multi_segment_path.baml:13:3 (keyword) len=8 "function"
// multi_segment_path.baml:13:12 (method) [declaration] len=5 "greet"
// multi_segment_path.baml:13:18 (parameter) [declaration] len=4 "self"
// multi_segment_path.baml:13:27 (type) [defaultLibrary] len=6 "string"
// multi_segment_path.baml:14:5 (parameter) len=4 "self"
// multi_segment_path.baml:14:10 (property) len=4 "name"
// multi_segment_path.baml:18:1 (keyword) len=8 "function"
// multi_segment_path.baml:18:10 (function) [declaration] len=11 "test_tokens"
// multi_segment_path.baml:18:27 (type) [defaultLibrary] len=6 "string"
// multi_segment_path.baml:19:3 (comment) len=64 "// Status.Active — \"Active\" should be enumMember, not property"
// multi_segment_path.baml:20:3 (keyword) len=3 "let"
// multi_segment_path.baml:20:7 (variable) [declaration] len=1 "s"
// multi_segment_path.baml:20:9 (operator) len=1 "="
// multi_segment_path.baml:20:11 (enum) len=6 "Status"
// multi_segment_path.baml:20:18 (enumMember) len=6 "Active"
// multi_segment_path.baml:22:3 (comment) len=60 "// user.greet() — \"greet\" should be function, not property"
// multi_segment_path.baml:23:3 (keyword) len=3 "let"
// multi_segment_path.baml:23:7 (variable) [declaration] len=1 "u"
// multi_segment_path.baml:23:9 (operator) len=1 "="
// multi_segment_path.baml:23:11 (class) len=4 "User"
// multi_segment_path.baml:23:18 (property) len=4 "name"
// multi_segment_path.baml:23:24 (string) len=4 "\"hi\""
// multi_segment_path.baml:24:3 (keyword) len=3 "let"
// multi_segment_path.baml:24:7 (variable) [declaration] len=1 "g"
// multi_segment_path.baml:24:9 (operator) len=1 "="
// multi_segment_path.baml:24:11 (variable) len=1 "u"
// multi_segment_path.baml:24:13 (method) len=5 "greet"
// multi_segment_path.baml:26:3 (comment) len=68 "// user.name — \"name\" should be property (this is correct already)"
// multi_segment_path.baml:27:3 (variable) len=1 "u"
// multi_segment_path.baml:27:5 (property) len=4 "name"
