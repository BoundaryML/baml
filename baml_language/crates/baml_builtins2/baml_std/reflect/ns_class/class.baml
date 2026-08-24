class Field {
  name string
  type reflect.Type?
  meta reflect.Meta
  /// Present only for handles returned from `baml.AnyClass.list_fields` or
  /// `get_field`. Type-side descriptors returned by `Type.fields()` are
  /// intentionally ownerless.
  _owner unknown?

  function metadata(self) -> reflect.Meta throws never {
    self.meta
  }

  /// Read this field from its bound owner. An ownerless type-side descriptor
  /// returns `null`.
  function value<T>(self) -> T? throws baml.errors.TypeMismatch {
    let owner: baml.AnyClass = self._owner else {
      return null
    }
    reflect.class.get_field<T>(owner, self.name) catch (e) {
      reflect.errors.CompilationError { message } => {
        throw baml.errors.TypeMismatch { message: message }
      }
    }
  }
}

/// Construct a closed runtime class. Map iteration order is definition order.
function new(
  name: string,
  fields: map<string, reflect.Type | reflect.WithMeta<reflect.Type>>,
  implementations: reflect.interface.Implementation[] = [],
) -> Type throws reflect.errors.CompilationError {
  // Native functions receive the omitted-argument sentinel instead of
  // evaluating BAML default expressions. Keep the public API as the BEP
  // specifies and resolve the default in this BAML wrapper before crossing
  // the native boundary.
  _new(name, fields, implementations)
}

/// Internal native backing `new`; all arguments have been materialized.
//baml:mut_vm
function _new(
  name: string,
  fields: map<string, reflect.Type | reflect.WithMeta<reflect.Type>>,
  implementations: reflect.interface.Implementation[],
) -> Type throws reflect.errors.CompilationError {
  $rust_function
}

/// Start an incrementally-defined runtime class. Builders are required when
/// one or more fields recursively refer to the class being defined.
//baml:mut_vm
function builder(name: string) -> Builder throws never {
  $rust_function
}

/// A mutable runtime-class definition. Calling `build` freezes every builder
/// connected through a pending field type as one atomic recursive group.
class Builder {
  _handle $rust_type
  _roots unknown[]
  _neighbors Builder[]
  _resolved reflect.Type?

  /// Append one field in definition order.
  ///
  /// A recursive field carries metadata the same way a resolved one does:
  /// `PendingType.meta(...)` produces the `WithMeta<PendingType>` row, and the
  /// metadata survives the atomic group build.
  //baml:mut_vm
  function field(
    self,
    name: string,
    ty: reflect.Type | reflect.WithMeta<reflect.Type> | PendingType | reflect.WithMeta<PendingType>,
  ) -> Builder throws reflect.errors.CompilationError {
    $rust_function
  }

  /// A statically-distinct reference to this builder's eventual class type.
  //baml:mut_vm
  function type(self) -> PendingType throws never {
    $rust_function
  }

  /// Validate and freeze this builder's complete connected recursive group.
  function build(
    self,
    implementations: reflect.interface.Implementation[] = [],
  ) -> Type throws reflect.errors.CompilationError {
    self._build(implementations)
  }

  /// Internal native backing `build`; all arguments have been materialized.
  //baml:mut_vm
  function _build(
    self,
    implementations: reflect.interface.Implementation[],
  ) -> Type throws reflect.errors.CompilationError {
    $rust_function
  }
}

/// A reference to a not-yet-frozen builder type. It is deliberately not a
/// subtype of the built-in `type`; only Builder.field accepts it directly.
class PendingType {
  _handle $rust_type
  _roots unknown[]
  _resolved reflect.Type?

  /// Construct an array over this pending type.
  //baml:mut_vm
  function array(self) -> PendingType throws never {
    $rust_function
  }

  /// Construct the canonical union of this pending type and null.
  //baml:mut_vm
  function optional(self) -> PendingType throws never {
    $rust_function
  }

  /// Construct a union containing this pending type and `others`.
  //baml:mut_vm
  function union(self, others: (reflect.Type | PendingType)[]) -> PendingType throws never {
    $rust_function
  }

  /// Return the frozen type, or null while any referenced builder is pending.
  //baml:mut_vm
  function resolved(self) -> reflect.Type? throws never {
    $rust_function
  }

  /// Pair this pending reference with schema metadata, mirroring `reflect.Type.meta`.
  /// Only `Builder.field` accepts the result.
  //baml:mut_vm
  function meta(
    self,
    alias: string? = null,
    description: string? = null,
    docstring: string? = null,
    other: map<string, string>? = null,
  ) -> reflect.WithMeta<PendingType> throws never {
    $rust_function
  }
}

/// Assert-read a named field from a runtime-shaped class instance.
//baml:mut_vm
function get_field<T>(value: unknown, name: string) -> T throws reflect.errors.CompilationError {
  $rust_function
}

/// A class-kind view of a `type` value. Users never construct this class.
class Type {
  implements reflect.TypeView {
    //baml:vm
    function as_type(self) -> reflect.Type throws never { $rust_function }
  }
  //baml:mut_vm
  function fields(self) -> Field[] throws never { $rust_function }
  //baml:mut_vm
  function meta(self) -> reflect.Meta throws never { $rust_function }
}
