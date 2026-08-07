/// Thrown when `baml.reflect.Package.compile` rejects submitted source.
class CompilationError {
  message string
  diagnostics baml.reflect.Diagnostic[]
}
