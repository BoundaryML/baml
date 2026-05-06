class InvalidArgument {
  message string
}

/// Thrown when text cannot be parsed into the requested type — e.g.
/// `int.parse("abc")` or `float.parse("")`.
class ParseError {
  message string
}

class Io {
  message string
}

class Timeout {
  message string
  duration_ms int?
}

class Unsupported {
  message string
}

class AccessError {
  message string
}

class RenderPrompt {
  message string
}

class NotImplemented {
  message string
}

class LlmClient {
  message string
}

class DevOther {
  message string
}

class HostPanic {
  message string
}
