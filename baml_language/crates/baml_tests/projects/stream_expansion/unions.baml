// Test stream expansion for unions and optionals.
// Expected: union fields get null | expanded_variant1 | expanded_variant2.
// Expected: optional fields expand T? as T | null.

class WithUnions {
  simple int | string
  with_class Inner | Primitives
  mixed int | Inner
}

class WithOptionals {
  name string?
  inner Inner?
  items int[]?
}
