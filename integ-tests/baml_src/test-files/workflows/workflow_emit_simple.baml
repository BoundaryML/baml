// Simple filter function that only notifies when value is not empty
function NotEmpty(prev: string, next: string) -> bool {
  next != ""
}

// Simple test without loops
function SimpleWatchWithFilter() -> int {
  let word: string = "" @watch(when=NotEmpty);

  word = "hello";  // Should notify (not empty)
  word = "world";  // Should notify (not empty)
  word = "";       // Should NOT notify (empty)
  word = "test";   // Should notify (not empty)

  42
}

test TestSimpleWatchWithFilter() {
  functions [SimpleWatchWithFilter]
  args {}
}
