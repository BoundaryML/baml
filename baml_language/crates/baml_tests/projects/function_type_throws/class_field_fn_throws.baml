// === Class fields with function types and throws ===

class PureHandler {
  run: () -> null
}

class ThrowingHandler {
  run: () -> null throws string
}

class MixedHandler {
  safe: () -> int
  risky: () -> int throws string
}

// Function returning a class with function fields
function make_pure_handler() -> PureHandler {
  PureHandler { run: () -> null { null } }
}

function make_throwing_handler() -> ThrowingHandler {
  ThrowingHandler { run: () -> null { throw "error" } }
}
