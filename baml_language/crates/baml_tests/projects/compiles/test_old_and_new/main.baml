// Old-style config-block test (should continue to work)
function Greet(name: string) -> string {
  "Hello, {name}!"
}

test OldStyle {
  functions [Greet]
  args {
    name "world"
  }
}

// New-style expression-body test
test "new style" {
  let result = "hello"
  assert.not_null(result)
}
