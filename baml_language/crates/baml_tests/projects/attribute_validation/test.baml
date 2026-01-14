// Test HIR validation for duplicate attributes

class HasDuplicateDescription {
  name string
  @@description("First description")
  @@alias("AKA")
  @@description("Second description")
}

class HasDuplicateField {
  name string
  age int
  name int
}

class HasDuplicateFieldAttr {
  name string @alias("n") @description("Name") @alias("nm")
}

enum HasDuplicateVariant {
  RED
  BLUE
  RED
}

enum HasDuplicateEnumAttr {
  ONE
  TWO
  @@description("First")
  @@description("Second")
}

enum HasDuplicateVariantAttr {
  ACTIVE @alias("a") @alias("active")
  INACTIVE
}

// Test: @alias requires a string literal, not an identifier/expression
class BadAliasValue {
  name string @alias(some_var)
}

// Test: @@dynamic takes no arguments
class BadDynamicArg {
  name string
  @@dynamic("unexpected")
}

// Test: @skip takes no arguments
class BadSkipArg {
  name string @skip("reason")
}
