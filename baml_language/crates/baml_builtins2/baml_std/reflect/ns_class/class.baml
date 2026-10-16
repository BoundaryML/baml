/// One field of a class: its name, its declared type, and its schema
/// metadata.
///
/// A descriptor from `reflect.AnyClass.list_fields`, `get_field` or
/// `find_field` is bound to the instance it came from, so `value` can read
/// it; one from `reflect.Class.fields()` describes the type only and is
/// ownerless.
class Field {
    name: string,
    type: root.Type?,
    meta: root.Meta,
    /// Present only for handles returned from `reflect.AnyClass.list_fields`,
    /// `get_field` or `find_field`. Type-side descriptors returned by
    /// `reflect.Class.fields()` are intentionally ownerless.
    _owner: unknown?,

    /// This field's schema metadata. Same value as the `meta` field.
    function metadata(self) -> root.Meta throws never {
        self.meta
    }

    /// Read this field from its bound owner. An ownerless type-side descriptor
    /// returns `null`.
    function value<T>(self) -> T? throws root.errors.TypeMismatch {
        let owner: root.AnyClass = self._owner else {
            return null;
        };
        root.Class.get_field<T>(owner, self.name) catch (e) {
            root.errors.CompilationError { message } => {
                throw root.errors.TypeMismatch { message: message }
            }
        }
    }
}

/// A mutable runtime-class definition. Calling `build` freezes every builder
/// connected through a pending field type as one atomic recursive group.
class Builder {
    _handle: $rust_type,
    _roots: unknown[],
    _neighbors: Builder[],
    _resolved: root.Type?,

    /// Append one field in definition order.
    ///
    /// A recursive field carries metadata the same way a resolved one does:
    /// `PendingType.meta(...)` produces the `WithMeta<PendingType>` row, and the
    /// metadata survives the atomic group build.
    //baml:mut_vm
    function field(
        self,
        name: string,
        ty: root.TypeView | root.WithMeta<root.Type> | PendingType | root.WithMeta<PendingType>,
    ) -> Builder throws root.errors.CompilationError {
        $rust_function
    }

    /// Append every field of `some_class`, in declaration order, with all of
    /// its metadata: alias, description, docstring, other and skip.
    ///
    /// Only the fields are copied. The new class gets neither `some_class`'s
    /// own metadata nor its interfaces; it implements what `build` is given.
    //baml:mut_vm
    function fields_from(
        self,
        some_class: root.Class,
    ) -> Builder throws root.errors.CompilationError {
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
        implementations: root.interface.Implementation[] = [],
    ) -> root.Class throws root.errors.CompilationError {
        self._build(implementations)
    }

    /// (internal) Internal native backing `build`; all arguments have been materialized.
    //baml:mut_vm
    function _build(
        self,
        implementations: root.interface.Implementation[],
    ) -> root.Class throws root.errors.CompilationError {
        $rust_function
    }
}

/// A reference to a not-yet-frozen builder type. It is deliberately not a
/// subtype of the built-in `type`; only Builder.field accepts it directly.
class PendingType {
    _handle: $rust_type,
    _roots: unknown[],
    _resolved: root.Type?,

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
    function union(self, others: (root.TypeView | PendingType)[]) -> PendingType throws never {
        $rust_function
    }

    /// Return the frozen type, or null while any referenced builder is pending.
    //baml:mut_vm
    function resolved(self) -> root.Type? throws never {
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
    ) -> root.WithMeta<PendingType> throws never {
        $rust_function
    }
}
