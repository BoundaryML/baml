// Simple filter function that only emits when value is not empty
function NotEmpty(prev: string, next: string) -> bool {
  next != ""
}

// Simple test without loops
function SimpleEmitWithFilter() -> int {
  let word: string = "" @emit(when=NotEmpty);

  word = "hello";  // Should emit (not empty)
  word = "world";  // Should emit (not empty)
  word = "";       // Should NOT emit (empty)
  word = "test";   // Should emit (not empty)

  42
}

test TestSimpleEmitWithFilter() {
  functions [SimpleEmitWithFilter]
  args {}
}
