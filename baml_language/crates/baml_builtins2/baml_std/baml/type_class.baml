/// Companion class for the built-in `type` primitive.
///
/// The BAML `type` primitive represents a runtime type value returned by
/// `type.of<T>()`. This class provides methods on `type` values,
/// mirroring how `class String` provides methods on `string`.
///
/// Users never construct `Type` directly; they receive `type` values via
/// `type.of<User>()` and call methods on them.
class TypeValue {
    /// Construct an array type whose element is this type.
    //baml:mut_vm
    function array(self) -> baml.reflect.array.Type throws never { $rust_function }

    /// Construct the canonical union of this type and null.
    //baml:mut_vm
    function optional(self) -> baml.reflect.union.Type throws never { $rust_function }

    /// Render deterministic canonical BAML source for this definition.
    //baml:vm
    function to_baml(self) -> string throws never { $rust_function }

    /// Returns the precise kind view of this type value. The view preserves
    /// the receiver's mint identity.
    //baml:vm
    function kind(self) -> baml.reflect.TypeKind throws never {
        $rust_function
    }

    //baml:vm
    function as_class(self) -> baml.reflect.class.Type? throws never { $rust_function }
    //baml:vm
    function as_enum(self) -> baml.reflect.enum.Type? throws never { $rust_function }
    //baml:vm
    function as_union(self) -> baml.reflect.union.Type? throws never { $rust_function }
    //baml:vm
    function as_literal(self) -> baml.reflect.literal.Type? throws never { $rust_function }
    //baml:vm
    function as_array(self) -> baml.reflect.array.Type? throws never { $rust_function }
    //baml:vm
    function as_map(self) -> baml.reflect.map.Type? throws never { $rust_function }
    //baml:vm
    function as_interface(self) -> baml.reflect.interface.Type? throws never { $rust_function }
    //baml:vm
    function as_primitive(self) -> baml.reflect.primitive.Type? throws never { $rust_function }
    //baml:vm
    function as_function(self) -> baml.reflect.function.Type? throws never { $rust_function }

    /// Internal shared-render-boundary guard for BEP-066 R-1 and R-3.
    //baml:mut_vm
    function _validate_renderable(self) -> void throws baml.reflect.errors.CompilationError {
        $rust_function
    }

    /// Pair this type value with schema metadata without changing its mint.
    //baml:mut_vm
    function meta(
      self,
      alias: string? = null,
      description: string? = null,
      docstring: string? = null,
      other: map<string, string>? = null,
    ) -> baml.reflect.WithMeta<type> throws never { $rust_function }

    /// Returns the type name as a human-readable string.
    implements baml.ToString {
        function to_string(self) -> string throws never {
            self._to_string_impl()
        }
    }

    //baml:vm
    function _to_string_impl(self) -> string throws never {
        $rust_function
    }

    /// True iff the receiver type nominally implements the interface `other`
    /// (BEP-044). Equivalent to: there is an `implements other { ... }` block
    /// on the receiver class, either directly or via an interface in the
    /// receiver's transitive `implements`/`extends` graph.
    ///
    /// Returns `false` when `other` is not an interface, or when the
    /// receiver is not a class.
    //baml:vm
    function implements(self, other: type) -> bool throws never {
        $rust_function
    }

    /// Reverse of [`implements`]: `iface.implemented_by(klass)` is the same
    /// as `klass.implements(iface)`. Useful when the call-site naturally
    /// reads "does this interface include that class?".
    //baml:vm
    function implemented_by(self, other: type) -> bool throws never {
        $rust_function
    }
}
