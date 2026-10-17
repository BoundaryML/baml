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
    function of<T>() -> root.Type {
        $compiler_intrinsic
    }

    /// Return the concrete runtime type of `value`, never a literal type.
    ///
    /// Values with no reconstructable BAML type produce the `unknown` type.
    //baml:mut_vm
    //baml:fallible
    function of_value(value: unknown) -> root.Type throws never {
        $rust_function
    }

    implements root.TypeView {
        function as_type(self) -> root.Type throws never {
            self
        }
    }

    /// Construct an array type whose element is this type.
    //baml:mut_vm
    function array(self) -> root.Array throws never {
        $rust_function
    }

    /// Construct the canonical union of this type and null.
    //baml:mut_vm
    function optional(self) -> root.Union throws never {
        $rust_function
    }

    /// Render deterministic canonical BAML source for this definition.
    /// Non-equivalent declarations with the same display name are rejected.
    //baml:mut_vm
    function to_baml(self) -> string throws root.errors.CompilationError {
        $rust_function
    }

    /// Returns the precise kind view of this type value: a freshly
    /// allocated view instance wrapping the receiver in its `_ty` field.
    //baml:mut_vm
    function kind(self) -> root.TypeKind throws never {
        $rust_function
    }

    /// The class view of this type, or `null` when it does not describe
    /// a class.
    //baml:mut_vm
    function as_class(self) -> root.Class? throws never {
        $rust_function
    }
    /// The enum view of this type, or `null` when it does not describe
    /// an enum.
    //baml:mut_vm
    function as_enum(self) -> root.Enum? throws never {
        $rust_function
    }
    /// The union view of this type, or `null` when it does not describe
    /// a union.
    //baml:mut_vm
    function as_union(self) -> root.Union? throws never {
        $rust_function
    }
    /// The literal view of this type, or `null` when it does not describe
    /// a literal.
    //baml:mut_vm
    function as_literal(self) -> root.Literal? throws never {
        $rust_function
    }
    /// The array view of this type, or `null` when it does not describe
    /// an array.
    //baml:mut_vm
    function as_array(self) -> root.Array? throws never {
        $rust_function
    }
    /// The map view of this type, or `null` when it does not describe
    /// a map.
    //baml:mut_vm
    function as_map(self) -> root.Map? throws never {
        $rust_function
    }
    /// The interface view of this type, or `null` when it does not describe
    /// an interface.
    //baml:mut_vm
    function as_interface(self) -> root.Interface? throws never {
        $rust_function
    }
    /// The primitive view of this type, or `null` when it does not describe
    /// a primitive.
    //baml:mut_vm
    function as_primitive(self) -> root.Primitive? throws never {
        $rust_function
    }
    /// The function view of this type, or `null` when it does not describe
    /// a function.
    //baml:mut_vm
    function as_function(self) -> root.Function? throws never {
        $rust_function
    }

    /// (internal) Internal shared-render-boundary guard for BEP-066 R-1 and R-3.
    //baml:mut_vm
    function _validate_renderable(self) -> void throws root.errors.CompilationError {
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
    ) -> root.WithMeta<root.Type> throws never {
        $rust_function
    }

    /// (internal) The rendering behind `baml.ToString` for every type value.
    /// `reflect.Type` and each kind view implement `baml.ToString` by
    /// out-of-body `implement` blocks beside the interface
    /// (baml/conversions.baml): `baml` depends on `reflect`, never the reverse.
    /// Code inside `reflect` cannot see those implementations, so it calls this
    /// method directly.
    //baml:vm
    function _to_string_impl(self) -> string throws never {
        $rust_function
    }

    /// True iff the receiver type nominally implements the interface `other`
    /// (BEP-044): an `implements other { ... }` block in the receiver's body,
    /// or an `implement other for ...` block anywhere in the program, applies
    /// to it with its bounds satisfied. The receiver can be any concrete type:
    /// a class, an enum, a primitive, a list, a map, ...
    ///
    /// `other` is an interface type, so it names every associated type the
    /// interface declares without a default, and an omitted one takes its
    /// default: `baml.iter.Iterator<Item = int>` is
    /// `baml.iter.Iterator<Item = int, Error = never>`. Only an
    /// implementation with exactly those associated types matches, so an int
    /// iterator that throws does not implement `Iterator<Item = int>`, and
    /// there is no way to ask "is this any iterator".
    ///
    /// Returns `false` when `other` is not an interface, or when the receiver
    /// is not a concrete type (an interface does not implement itself).
    function implements(self, other: root.TypeView) -> bool throws never {
        self._implements(other.as_type())
    }

    /// Reverse of [`implements`]: `iface.implemented_by(t)` is the same as
    /// `t.implements(iface)`, with the same associated-type rule. Useful when
    /// the call site naturally reads "does this interface include that type?".
    function implemented_by(self, other: root.TypeView) -> bool throws never {
        other.as_type()._implements(self)
    }

    /// (internal) Internal native backing `implements` and `implemented_by`;
    /// both sides are `type` values.
    //baml:vm
    function _implements(self, other: root.Type) -> bool throws never {
        $rust_function
    }
}
