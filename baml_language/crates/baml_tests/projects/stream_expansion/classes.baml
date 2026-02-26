// Test stream expansion for class-referencing fields.
// Expected: class fields get null | stream_ClassName.

class Inner {
  value string
}

class Outer {
  inner Inner
}
