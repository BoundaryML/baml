/// Returns a deep copy of `value` — all nested structures are recursively cloned.
//baml:mut_vm
function deep_copy<T>(value: T) -> T throws never {
    $rust_function
}

/// Implemented by all concrete types.
/// Can be used to check or bound a type to only include concrete types.
interface Concrete {

}

implement<T> Concrete for T {

}

/// Implemented automatically by every function value, with its parameter
/// list erased. `Returns` pins the held function's return type and `Throws`
/// its error type; both default to `unknown`, so a bare `AnyFunction`
/// accepts any function. A compiler builtin: it cannot be implemented by
/// hand, cannot be used as a generic bound, and declares no methods. Call
/// the held function through `reflect.call_any`; read its signature with
/// `reflect.signature`.
interface AnyFunction {
  type Returns = unknown
  type Throws = unknown
}

/// A read-only reflected view implemented automatically by class values.
///
/// Narrow an opaque value with `let value: baml.AnyClass = opaque else { ... }`
/// before using this surface. Ordinary class instances implement it, including
/// runtime-minted classes. Of the sealed reflection-kind views, only
/// `baml.reflect.class.Type` implements it.
interface AnyClass {
  /// Read a field as `T`. A missing field returns `null`; use `has_field` to
  /// distinguish that case from a present field whose value is null.
  function get<T>(self, name: string) -> T? throws baml.errors.TypeMismatch {
    if (!self.has_field(name)) {
      return null
    }
    // NOTE: spelled with `let … else` rather than `self.get_field(name)?.value<T>()`
    // because an optional-chained call does not thread `T` to the callee.
    let field: baml.reflect.class.Field = self.get_field(name) else {
      return null
    }
    field.value<T>()
  }

  /// Return a value-bound field handle, or `null` when no such field exists.
  function get_field(self, name: string) -> baml.reflect.class.Field? throws never {
    let class_type: baml.reflect.class.Type = type.of_value(self).as_class() else {
      return null
    }
    for (let field in class_type.fields()) {
      if (field.name == name) {
        return baml.reflect.class.Field {
          name: field.name,
          type: field.type,
          meta: field.meta,
          _owner: self,
        }
      }
    }
    null
  }

  function has_field(self, name: string) -> bool throws never {
    let class_type: baml.reflect.class.Type = type.of_value(self).as_class() else {
      return false
    }
    for (let field in class_type.fields()) {
      if (field.name == name) {
        return true
      }
    }
    false
  }

  /// Reflected fields bound to this value for `Field.value<T>()`.
  function list_fields(self) -> baml.reflect.class.Field[] throws never {
    let class_type: baml.reflect.class.Type = type.of_value(self).as_class() else {
      return []
    }
    let fields: baml.reflect.class.Field[] = []
    for (let field in class_type.fields()) {
      fields.push(baml.reflect.class.Field {
        name: field.name,
        type: field.type,
        meta: field.meta,
        _owner: self,
      })
    }
    fields
  }

  function name(self) -> string throws never {
    type.of_value(self).to_string()
  }

  function type(self) -> type throws never {
    type.of_value(self)
  }

  function attributes(self) -> baml.reflect.Meta throws never {
    let class_type: baml.reflect.class.Type = type.of_value(self).as_class() else {
      return baml.reflect.Meta {
        alias: null,
        description: null,
        docstring: null,
        other: {},
      }
    }
    class_type.meta()
  }
}

implement<T> AnyClass for T {}

/// The structured value a tagged template literal produces.
///
/// BEP-049 §10. A `tag` ` `…` ` call site lowers to `tag(body = () -> TaggedString { … })`;
/// when `body()` runs it returns this struct. The invariant is
/// `parts.length == values.length + 1` (literals and values alternate, starting and
/// ending with a literal — empty strings fill the leading/trailing slot when the
/// template begins or ends with an interpolation).
class TaggedString {
    /// Literal text segments between interpolations, in source order.
    parts: string[],
    /// Interpolated values, in source order. Heterogeneous: each entry has the
    /// type of its `${expr}` site; tags inspect element types at runtime.
    values: unknown[],
}
