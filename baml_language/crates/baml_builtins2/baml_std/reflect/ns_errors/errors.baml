/// A reflection construction failed compiler-equivalent validation.
class CompilationError {
  message string
  diagnostics reflect.Diagnostic[]
}

/// Runtime evaluation failed after successful construction/compilation.
class EvaluationError {
  message string
}

/// A reflective typed read did not fit the requested type: `Field.value<T>()`
/// or `AnyClass.get<T>()` asked for a `T` the underlying value is not.
///
/// Distinct from `baml.errors.TypeMismatch`, the host call-boundary error
/// synthesized from `EngineError::TypeMismatch` — that one names the FFI
/// contract; this one names a reflection read.
class TypeMismatch {
  message string
}

/// A session rejected concurrent use.
class SessionBusy {
  message string
}
