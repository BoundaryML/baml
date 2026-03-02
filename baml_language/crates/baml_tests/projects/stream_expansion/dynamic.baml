// Test stream expansion with @@dynamic classes.
// Expected: stream_DynamicClass is also marked @@dynamic.

class DynamicClass {
  name string
  @@dynamic
}
