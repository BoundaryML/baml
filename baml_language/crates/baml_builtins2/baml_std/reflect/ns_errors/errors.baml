/// A reflection construction failed compiler-equivalent validation.
class CompilationError {
    message: string,
    diagnostics: root.Diagnostic[],
}

/// Runtime evaluation failed after successful construction/compilation.
class EvaluationError {
    message: string,
    cause: unknown,
}

/// A lookup by name found nothing: a `get_*` lookup on a `reflect.Package`
/// (`get_class`, `get_enum`, `get_interface`, `get_function`) or on a value
/// (`AnyClass.get`, `AnyClass.get_field`). `message` names what was missing
/// and its kind, for example `no class named Foo in this package`. Each such
/// lookup has a `find_*` counterpart (`AnyClass.find` for `get`) that returns
/// `null` instead.
class NotFound {
    message: string,
}

/// A reflective typed read did not fit the requested type: `Field.value<T>()`,
/// `AnyClass.get<T>()`, or `AnyClass.find<T>()` asked for a `T` the
/// underlying value is not.
///
/// Distinct from `baml.errors.TypeMismatch`, the host call-boundary error
/// synthesized from `EngineError::TypeMismatch` — that one names the FFI
/// contract; this one names a reflection read.
class TypeMismatch {
    message: string,
}

/// A reflection call got an argument it cannot accept: `Enum.get_value` given
/// a value that is not an enum value.
///
/// The `reflect` counterpart of `baml.errors.InvalidArgument`, with the same
/// shape: `reflect` does not depend on `baml`, so it cannot name that class.
class InvalidArgument {
    message: string,
}

/// A session rejected concurrent use.
class SessionBusy {
    message: string,
}
