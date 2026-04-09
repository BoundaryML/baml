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

class MethodRunner<T> {
  value: T

  function apply(self, f: (T) -> int) -> int {
    f(self.value)
  }

  function apply_underdeclared(self, f: (T) -> int) -> int throws never {
    f(self.value)
  }
}

// Function returning a class with function fields
function make_pure_handler() -> PureHandler {
  PureHandler { run: () -> null { null } }
}

function make_throwing_handler() -> ThrowingHandler {
  ThrowingHandler { run: () -> null { throw "error" } }
}

function test_method_runner() -> int {
  let runner = MethodRunner<int> { value: 1 }
  runner.apply((x: int) -> int { x + 1 })
}
