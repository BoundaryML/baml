/// Companion class for the built-in `type` primitive.
///
/// The BAML `type` primitive represents a runtime type value returned by
/// `reflect.type_of<T>()`. This class provides methods on `type` values,
/// mirroring how `class String` provides methods on `string`.
///
/// Users never construct `Type` directly; they receive `type` values via
/// `reflect.type_of<User>()` and call methods on them.
class TypeValue {
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
