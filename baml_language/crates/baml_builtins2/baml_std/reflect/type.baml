/// A runtime value describing a BAML type.
///
/// `reflect.Type.of<T>()` describes a type argument and
/// `reflect.Type.of_value(value)` describes a value's concrete runtime type.
/// A type value's identity is its declarations': two values naming the same
/// declarations compare equal, and a runtime-created declaration is equal only
/// to itself. Reflection methods are exposed directly.
class Type {
    /// Return the runtime type value for the type argument `T`.
    ///
    /// Concrete arguments are materialized by one `LoadType` instruction.
    function of<T>() -> reflect.Type {
        $compiler_intrinsic
    }

    /// Return the concrete runtime type of `value`, never a literal type.
    ///
    /// Values with no reconstructable BAML type produce the `unknown` type.
    //baml:mut_vm
    //baml:fallible
    function of_value(value: unknown) -> reflect.Type throws never {
        $rust_function
    }

    /// Construct an array type whose element is this type.
    //baml:mut_vm
    function array(self) -> reflect.array.Type throws never {
        $rust_function
    }

    /// Construct the canonical union of this type and null.
    //baml:mut_vm
    function optional(self) -> reflect.union.Type throws never {
        $rust_function
    }

    /// Render deterministic canonical BAML source for this definition.
    /// Non-equivalent declarations with the same display name are rejected.
    //baml:mut_vm
    function to_baml(self) -> string throws reflect.errors.CompilationError {
        $rust_function
    }

    /// Returns the precise kind view of this type value: a freshly
    /// allocated view instance wrapping the receiver in its `_ty` field.
    //baml:mut_vm
    function kind(self) -> reflect.TypeKind throws never {
        $rust_function
    }

    //baml:mut_vm
    function as_class(self) -> reflect.class.Type? throws never {
        $rust_function
    }
    //baml:mut_vm
    function as_enum(self) -> reflect.enum.Type? throws never {
        $rust_function
    }
    //baml:mut_vm
    function as_union(self) -> reflect.union.Type? throws never {
        $rust_function
    }
    //baml:mut_vm
    function as_literal(self) -> reflect.literal.Type? throws never {
        $rust_function
    }
    //baml:mut_vm
    function as_array(self) -> reflect.array.Type? throws never {
        $rust_function
    }
    //baml:mut_vm
    function as_map(self) -> reflect.map.Type? throws never {
        $rust_function
    }
    //baml:mut_vm
    function as_interface(self) -> reflect.interface.Type? throws never {
        $rust_function
    }
    //baml:mut_vm
    function as_primitive(self) -> reflect.primitive.Type? throws never {
        $rust_function
    }
    //baml:mut_vm
    function as_function(self) -> reflect.function.Type? throws never {
        $rust_function
    }

    /// Internal shared-render-boundary guard for BEP-066 R-1 and R-3.
    //baml:mut_vm
    function _validate_renderable(self) -> void throws reflect.errors.CompilationError {
        $rust_function
    }

    /// Pair this type value with schema metadata without changing its identity.
    //baml:mut_vm
    function meta(
        self,
        alias: string? = null,
        description: string? = null,
        docstring: string? = null,
        other: map<string, string>? = null,
    ) -> reflect.WithMeta<reflect.Type> throws never {
        $rust_function
    }

    // `baml.ToString` is implemented for `reflect.Type` by an out-of-body
    // `implement` block beside the interface (baml/conversions.baml): the
    // `reflect` package deliberately references nothing from `baml`, so the
    // stdlib dependency graph stays acyclic by construction.
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
    function implements(self, other: reflect.Type) -> bool throws never {
        $rust_function
    }

    /// Reverse of [`implements`]: `iface.implemented_by(klass)` is the same
    /// as `klass.implements(iface)`. Useful when the call-site naturally
    /// reads "does this interface include that class?".
    //baml:vm
    function implemented_by(self, other: reflect.Type) -> bool throws never {
        $rust_function
    }
}
