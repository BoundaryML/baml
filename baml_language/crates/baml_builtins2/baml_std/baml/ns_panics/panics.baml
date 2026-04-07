class DivisionByZero {
  dividend int
}

class IndexOutOfBounds {
  index int
  length int
}

class MapKeyNotFound {
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

type Panic = DivisionByZero | IndexOutOfBounds | MapKeyNotFound | StackOverflow | AssertionFailed | Unreachable
