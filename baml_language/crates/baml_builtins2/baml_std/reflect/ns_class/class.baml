/// One field of a class: its name, its declared type, and its schema
/// metadata.
///
/// A descriptor from `reflect.AnyClass.list_fields` or `get_field` is bound
/// to the instance it came from, so `value` can read it; one from
/// `Type.fields()` describes the type only and is ownerless.
class Field {
    name: string,
    type: root.Type?,
    meta: root.Meta,
    /// Present only for handles returned from `reflect.AnyClass.list_fields` or
    /// `get_field`. Type-side descriptors returned by `Type.fields()` are
    /// intentionally ownerless.
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
        root.class.get_field<T>(owner, self.name) catch (e) {
            root.errors.CompilationError { message } => {
                throw root.errors.TypeMismatch { message: message }
            }
        }
    }
}

/// Construct a closed runtime class. Map iteration order is definition order.
function new(
    name: string,
    fields: map<string, root.Type | root.WithMeta<root.Type>>,
    implementations: root.interface.Implementation[] = [],
) -> Type throws root.errors.CompilationError {
    // Native functions receive the omitted-argument sentinel instead of
    // evaluating BAML default expressions. Keep the public API as the BEP
    // specifies and resolve the default in this BAML wrapper before crossing
    // the native boundary.
    _new(name, fields, implementations)
}

/// (internal) Internal native backing `new`; all arguments have been materialized.
//baml:mut_vm
function _new(
    name: string,
    fields: map<string, root.Type | root.WithMeta<root.Type>>,
    implementations: root.interface.Implementation[],
) -> Type throws root.errors.CompilationError {
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
        ty: root.Type | root.WithMeta<root.Type> | PendingType | root.WithMeta<PendingType>,
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
    ) -> Type throws root.errors.CompilationError {
        self._build(implementations)
    }

    /// (internal) Internal native backing `build`; all arguments have been materialized.
    //baml:mut_vm
    function _build(
        self,
        implementations: root.interface.Implementation[],
    ) -> Type throws root.errors.CompilationError {
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
    function union(self, others: (root.Type | PendingType)[]) -> PendingType throws never {
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

/// Assert-read a named field from a runtime-shaped class instance.
//baml:mut_vm
function get_field<T>(value: unknown, name: string) -> T throws root.errors.CompilationError {
    $rust_function
}

/// Whether the field `name` of the class instance `value` has been given a
/// value, `null` included.
///
/// An optional (`T?`) field has three states. It is absent when nothing gave
/// it a value: the class constructor left it out, the JSON object it was
/// decoded from had no such member, a host passed the object without it, or
/// `unset_field` cleared it. It is `null` when it was given `null` — written
/// `name: null`, assigned, or decoded from an explicit `null`. Otherwise it
/// holds a value. Reading an absent field gives `null`, so a read alone
/// cannot tell the first two apart; this can. `baml.json.from<T>` with
/// `omit_absent = true` leaves absent fields out of the JSON it writes.
///
/// A field that is not optional is always set. `==` does not look at
/// presence: an absent field compares equal to one holding `null`.
///
/// # Throws
///
/// - `reflect.errors.CompilationError` if `value` is not a class instance, or its
///   class has no field `name`.
///
/// # Examples
///
/// ```baml
/// class Patch { name: string?, expires_in: int? }
///
/// let p = baml.json.from_string<Patch>("{\"expires_in\": null}");
/// assert.equal(reflect.class.is_field_set(p, "name"), false);       // absent
/// assert.equal(reflect.class.is_field_set(p, "expires_in"), true);  // null
/// assert.equal(p.name, null);
/// ```
//baml:mut_vm
function is_field_set(value: unknown, name: string) -> bool throws root.errors.CompilationError {
    $rust_function
}

/// Make the optional field `name` of the class instance `value` absent again,
/// as if the constructor had left it out. It then reads as `null`, and
/// `is_field_set` reports `false`. Assigning the field, even `null`, sets it
/// again.
///
/// # Throws
///
/// - `reflect.errors.CompilationError` if `value` is not a class instance, its
///   class has no field `name`, or the field's type does not admit `null`.
//baml:mut_vm
function unset_field(value: unknown, name: string) -> void throws root.errors.CompilationError {
    $rust_function
}

/// A class-kind view of a `type` value. Users never construct this class.
class Type {
    /// The wrapped `type` value. Private by invariant: reflection natives are
    /// its only readers, and each kind method panics if the held type no longer
    /// classifies as this view's kind.
    _ty: root.Type,

    implements root.TypeView {
        //baml:vm
        //baml:fallible
        function as_type(self) -> root.Type throws never {
            $rust_function
        }
    }
    /// The class's fields, in declaration order. Ownerless: these describe
    /// the type, so `Field.value` on them returns `null`.
    //baml:mut_vm
    //baml:fallible
    function fields(self) -> Field[] throws never {
        $rust_function
    }
    /// The class declaration's own schema metadata.
    //baml:mut_vm
    //baml:fallible
    function meta(self) -> root.Meta throws never {
        $rust_function
    }
}
