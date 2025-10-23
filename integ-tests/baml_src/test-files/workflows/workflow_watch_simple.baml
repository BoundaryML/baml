// Simple filter function that only notifies when value is not empty
function NotEmpty(value: string) -> bool {
  value != ""
}

// Simple test without loops
function SimpleWatchWithFilter() -> int {
  watch let word: string = "";
  word.$watch.options(baml.WatchOptions{ when: NotEmpty });

  word = "hello";  // Should notify (not empty)
  word = "world";  // Should notify (not empty)
  word = "";       // Should NOT notify (empty)
  word = "test";   // Should notify (not empty)

  word.$watch.options(baml.WatchOptions{ channel: "new_name" });
  word = "with_new_name";   // Should notify (not empty)
  word.$watch.notify();

  42
}

test TestSimpleWatchWithFilter() {
  functions [SimpleWatchWithFilter]
  args {}
}
