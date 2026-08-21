class Parameter {
  name string?
  type type
  optional bool
}

/// One type parameter a generic function still expects.
///
/// Declared bounds are deliberately not part of this row yet: a bound may
/// mention the function's own parameters (`T extends Comparable<T>`), and such
/// a bound has no `type` value to hand back. `specialize` enforces every bound
/// regardless.
class GenericParam {
  /// The name it was declared with, without its bound (`T`).
  name string
}

/// A function-kind view of a `type` value. Throws are intentionally not part
/// of the function reflection surface.
///
/// An entry of `baml.reflect.Package.functions()` is also a *descriptor*: it
/// remembers the callable it was reflected from, so a generic one can be
/// specialized and the result extracted without a second lookup by name (a
/// specialized descriptor is anonymous). A function type obtained any other
/// way — `type.of<(int) -> int>()`, a field's type — carries no callable, so
/// `is_generic` is false, `generic_params` is empty, and `get` returns null.
class Type {
  implements baml.reflect.TypeView {
    //baml:vm
    function as_type(self) -> type throws never { $rust_function }
  }
  /// The declared parameters, in order.
  ///
  /// An unspecialized generic function whose surface mentions its own type
  /// parameters has no realized signature at all; reading one throws rather
  /// than reporting an invented shape. `specialize` first.
  //baml:mut_vm
  function params(self) -> Parameter[] throws baml.reflect.errors.CompilationError { $rust_function }
  /// The declared return type. Throws for an unspecialized generic, like `params`.
  //baml:mut_vm
  function return_type(self) -> type throws baml.reflect.errors.CompilationError { $rust_function }

  /// Whether this descriptor still expects type arguments.
  ///
  /// True for a declared generic function and for each of its generated
  /// companions (`Extract$render_prompt` is specialized separately); false for
  /// a concrete function, for a companion of one, and once `specialize` has
  /// supplied every argument.
  //baml:vm
  function is_generic(self) -> bool throws never { $rust_function }

  /// The type parameters still awaiting arguments, in declaration order.
  /// Empty whenever `is_generic` is false.
  //baml:mut_vm
  function generic_params(self) -> GenericParam[] throws never { $rust_function }

  /// Bind this descriptor's type parameters, returning a specialized
  /// descriptor whose signature is fully realized.
  ///
  /// `args` must supply exactly one type per `generic_params` entry, and each
  /// must satisfy that parameter's declared interface bounds; either failure
  /// throws. Specializing a function that is not generic throws too —
  /// `is_generic` is the guard. The supplied values are carried by reference,
  /// so a runtime-minted type keeps its identity and its definitions all the
  /// way into the specialized callable's body.
  //baml:mut_vm
  function specialize(self, args: type[]) -> Type throws baml.reflect.errors.CompilationError { $rust_function }

  /// Extract the callable behind this descriptor through the function-type
  /// contract `F`, exactly as `baml.reflect.Package.get_function` does for a
  /// named function: null when there is no callable, a throw when the
  /// callable's type is not a subtype of `F`.
  ///
  /// This is the only way to reach a specialized callable, which has no name
  /// to look up.
  //baml:mut_vm
  function get<F>(self) -> F? throws baml.reflect.errors.CompilationError { $rust_function }
}
