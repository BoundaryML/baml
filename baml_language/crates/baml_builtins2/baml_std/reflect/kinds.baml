/// A class-kind view of a `type` value. Users never construct this class.
class Class {
    /// The wrapped `type` value. Private by invariant: reflection natives are
    /// its only readers, and each kind method panics if the held type no longer
    /// classifies as this view's kind.
    _ty: root.Type,

    /// Construct a closed runtime class. Map iteration order is definition order.
    function new(
        name: string,
        fields: map<string, root.Type | root.WithMeta<root.Type>>,
        implementations: root.interface.Implementation[] = [],
    ) -> root.Class throws root.errors.CompilationError {
        // Native functions receive the omitted-argument sentinel instead of
        // evaluating BAML default expressions. Keep the public API as the BEP
        // specifies and resolve the default in this BAML wrapper before crossing
        // the native boundary.
        Class._new(name, fields, implementations)
    }

    /// (internal) Internal native backing `new`; all arguments have been materialized.
    //baml:mut_vm
    function _new(
        name: string,
        fields: map<string, root.Type | root.WithMeta<root.Type>>,
        implementations: root.interface.Implementation[],
    ) -> root.Class throws root.errors.CompilationError {
        $rust_function
    }

    /// Start an incrementally-defined runtime class. Builders are required when
    /// one or more fields recursively refer to the class being defined.
    //baml:mut_vm
    function builder(name: string) -> root.class.Builder throws never {
        $rust_function
    }

    /// Assert-read a named field from a runtime-shaped class instance.
    //baml:mut_vm
    function get_field<T>(value: unknown, name: string) -> T throws root.errors.CompilationError {
        $rust_function
    }

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
    function fields(self) -> root.class.Field[] throws never {
        $rust_function
    }
    /// The class declaration's own schema metadata.
    //baml:mut_vm
    //baml:fallible
    function meta(self) -> root.Meta throws never {
        $rust_function
    }
}

/// An enum-kind view of a `type` value. Users never construct this class.
class Enum {
    /// The wrapped `type` value. Private by invariant: reflection natives are
    /// its only readers, and each kind method panics if the held type no longer
    /// classifies as this view's kind.
    _ty: root.Type,

    /// Construct one enum row with optional schema metadata.
    //baml:mut_vm
    function value(
        name: string,
        alias: string? = null,
        description: string? = null,
        docstring: string? = null,
        other: map<string, string>? = null,
    ) -> root.enum.Value throws never {
        $rust_function
    }

    /// Mint a fresh runtime enum definition. Every evaluation has distinct type
    /// identity even when its name and rows are byte-identical.
    //baml:mut_vm
    //baml:fallible
    function new(
        name: string,
        values: (string | root.enum.Value)[],
    ) -> root.Enum throws root.errors.CompilationError {
        $rust_function
    }

    /// Recover the source variant name from a runtime or static enum value.
    ///
    /// # Throws
    /// - `reflect.errors.InvalidArgument` if `value` is not an enum value. The
    ///   parameter is `unknown` by design — the check is a runtime one, so a
    ///   caller that passes a non-enum gets an error value rather than a panic.
    //baml:mut_vm
    function get_value(value: unknown) -> string throws root.errors.InvalidArgument {
        $rust_function
    }

    implements root.TypeView {
        //baml:vm
        //baml:fallible
        function as_type(self) -> root.Type throws never {
            $rust_function
        }
    }
    //baml:mut_vm
    //baml:fallible
    /// The enum's variants, in declaration order.
    function values(self) -> root.enum.Value[] throws never {
        $rust_function
    }
    //baml:mut_vm
    //baml:fallible
    /// The enum declaration's own schema metadata.
    function meta(self) -> root.Meta throws never {
        $rust_function
    }
}

/// A union-kind view of a `type` value. Users never construct this class.
class Union {
    /// The wrapped `type` value. Private by invariant: reflection natives are
    /// its only readers, and each kind method panics if the held type no longer
    /// classifies as this view's kind.
    _ty: root.Type,

    /// Constructs the canonical union of `types`.
    ///
    /// # Throws
    ///
    /// - `reflect.errors.CompilationError` if the members cannot form a union.
    //baml:mut_vm
    function new(types: root.Type[]) -> root.Union throws root.errors.CompilationError {
        $rust_function
    }

    implements root.TypeView {
        //baml:vm
        //baml:fallible
        function as_type(self) -> root.Type throws never {
            $rust_function
        }
    }
    /// The union's members, in canonical order.
    //baml:mut_vm
    //baml:fallible
    function member_types(self) -> root.Type[] throws never {
        $rust_function
    }
}

/// A literal-kind view of a `type` value. Users never construct this class.
class Literal {
    /// The wrapped `type` value. Private by invariant: reflection natives are
    /// its only readers, and each kind method panics if the held type no longer
    /// classifies as this view's kind.
    _ty: root.Type,

    /// Constructs the literal type matching exactly a string, int, bigint, or bool value.
    //baml:mut_vm
    function new(value: string | int | bigint | bool) -> root.Literal throws never {
        $rust_function
    }

    implements root.TypeView {
        //baml:vm
        //baml:fallible
        function as_type(self) -> root.Type throws never {
            $rust_function
        }
    }

    /// Returns the string, int, bigint, bool, or enum variant represented by this literal type.
    /// The return type is unknown because enum variants can belong to any enum.
    /// This API cannot return class values today.
    //baml:mut_vm
    //baml:fallible
    function value(self) -> unknown throws never {
        $rust_function
    }
}

/// An array-kind view of a `type` value. Users never construct this class.
class Array {
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
    /// The type of the array's elements.
    //baml:mut_vm
    //baml:fallible
    function element_type(self) -> root.Type throws never {
        $rust_function
    }
}

/// A map-kind view of a `type` value. Users never construct this class.
class Map {
    /// The wrapped `type` value. Private by invariant: reflection natives are
    /// its only readers, and each kind method panics if the held type no longer
    /// classifies as this view's kind.
    _ty: root.Type,

    /// Constructs the map type `map<key, value>`.
    //baml:mut_vm
    function new(key: root.Type, value: root.Type) -> root.Map throws never {
        $rust_function
    }

    implements root.TypeView {
        //baml:vm
        //baml:fallible
        function as_type(self) -> root.Type throws never {
            $rust_function
        }
    }
    /// The type of the map's keys.
    //baml:mut_vm
    //baml:fallible
    function key_type(self) -> root.Type throws never {
        $rust_function
    }
    /// The type of the map's values.
    //baml:mut_vm
    //baml:fallible
    function value_type(self) -> root.Type throws never {
        $rust_function
    }
}

/// An interface-kind view of a `type` value. Users never construct this class.
class Interface {
    /// The wrapped `type` value. Private by invariant: reflection natives are
    /// its only readers, and each kind method panics if the held type no longer
    /// classifies as this view's kind.
    _ty: root.Type,

    /// Start a structural witness for a static interface.
    //baml:mut_vm
    function implementation<I>(
    ) -> root.interface.Implementation throws root.errors.CompilationError {
        $rust_function
    }

    implements root.TypeView {
        //baml:vm
        //baml:fallible
        function as_type(self) -> root.Type throws never {
            $rust_function
        }
    }
    //baml:vm
    //baml:fallible
    /// Whether `other` nominally implements this interface.
    ///
    /// The mirror of `reflect.Type.implements`, asked from the interface's
    /// side. `false` when `other` is not a class.
    function implemented_by(self, other: root.Type) -> bool throws never {
        $rust_function
    }
}

/// A primitive-kind view of a `type` value. Users never construct this class.
class Primitive {
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
}

/// A function-kind view of a `type` value. Throws are intentionally not part
/// of the function reflection surface.
class Function {
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
    //baml:mut_vm
    //baml:fallible
    /// The function's parameters, in declaration order.
    function params(self) -> root.function.Parameter[] throws never {
        $rust_function
    }
    //baml:mut_vm
    //baml:fallible
    /// The function's return type.
    function return_type(self) -> root.Type throws never {
        $rust_function
    }
}
