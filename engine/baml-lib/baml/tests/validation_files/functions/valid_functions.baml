function One {
  input string
  output string
}

class Hello {
  world string
}

class Hello2 {
  /// Comment about this
  world string // Comment about this
  /// This works
  // This works
  relevant_phrase_from_user_document string        // This breaks
  // test
  @alias(foo)
}

function Two {
  input Hello
  output Hello2
}
