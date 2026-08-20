/// A reflection construction failed compiler-equivalent validation.
class CompilationError {
  message string
  diagnostics reflect.Diagnostic[]
}

/// Runtime evaluation failed after successful construction/compilation.
class EvaluationError {
  message string
}

/// A session rejected concurrent use.
class SessionBusy {
  message string
}
