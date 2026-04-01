class DivisionByZero {
  dividend int
}

class IndexOutOfBounds {
  index int
  length int
}

class NegativeIndex {
  index int
}

class KeyNotFound {
  key string
}

class StackOverflow {
  message string
}

class AssertionFailed {
  message string
}

class Unreachable {
  message string
}

type Panic = DivisionByZero | IndexOutOfBounds | NegativeIndex | KeyNotFound | StackOverflow | AssertionFailed | Unreachable
