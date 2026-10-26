/// A class-kind view of a `type` value. Users never construct this class.
class Class {
    /// The wrapped `type` value. Private by invariant: reflection natives are
    /// its only readers, and each kind method panics if the held type no longer
    /// classifies as this view's kind.
    _ty: root.Type,

    /// Construct a closed runtime class. Map iteration order is definition order.
    function new(
        name: string,
        fields: map<string, root.TypeView | root.WithMeta<root.Type>>,
        implementations: root.interface.Implementation[] = [],
    ) -> root.Class throws root.errors.CompilationError {
        // Native functions receive the omitted-argument sentinel instead of
        // evaluating BAML default expressions. Keep the public API as the BEP
        // specifies and resolve the default in this BAML wrapper before crossing
        // the native boundary.
        Class._new(name, root._as_field_rows(fields), implementations)
    }

    /// (internal) Internal native backing `new`; all arguments have been
    /// materialized and every field type is a `type` value.
    //baml:mut_vm
    function _new(
        name: string,
        fields: map<string, root.Type | root.WithMeta<root.Type>>,
        implementations: root.interface.Implementation[],
    ) -> root.Class throws root.errors.CompilationError {
        $rust_function
    }

    /// Construct a closed runtime class that implements the interface `I`.
    ///
    /// The class declares every field of `I` and of the interfaces `I`
    /// requires, transitively, followed by `fields`. A required interface's
    /// fields come before the fields of the interface that requires it, and
    /// required interfaces are visited in the order their `requires` clause
    /// lists them, so for `interface FooBar requires HasA, HasB { c: int }`
    /// the class declares `a`, `b`, `c`, then `fields`. An interface reached
    /// through two paths contributes its fields once, at their first position.
    /// Each interface field keeps its doc comment.
    ///
    /// The class implements `I` and every interface in its `requires` closure,
    /// each interface field satisfied by the class field of the same name.
    /// Interface default methods apply to the class as usual.
    ///
    /// # Throws
    ///
    /// - `reflect.errors.CompilationError` if `I` is not an interface, if an
    ///   interface in the closure has a method without a default body, if
    ///   `fields` names a field an interface already declares, or if two
    ///   interfaces declare the same field with different types.
    function extends<I>(
        name: string,
        fields: map<string, root.TypeView | root.WithMeta<root.Type>>,
    ) -> root.Class throws root.errors.CompilationError {
        Class._extends<I>(name, root._as_field_rows(fields))
    }

    /// (internal) Internal native backing `extends`; every field type is a
    /// `type` value.
    //baml:mut_vm
    function _extends<I>(
        name: string,
        fields: map<string, root.Type | root.WithMeta<root.Type>>,
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

    /// Builds an instance of the class `T` from its field values.
    ///
    /// `T` is a class, or a generic class with its type arguments, such as
    /// `Box<int>`. It may be a class built at runtime and named with
    /// `let type`. Keys are the declared field names: `@alias` and `@skip`
    /// do not apply. Each value is stored as given, not copied or converted:
    /// an enum value stays an enum value, and a string with the same text
    /// stays a string. A field left out of `fields` holds `null` when its
    /// type accepts `null`.
    ///
    /// # Parameters
    ///
    /// - `fields`: each field's value, keyed by the field's declared name.
    ///
    /// # Returns
    ///
    /// A new instance of `T`.
    ///
    /// # Throws
    ///
    /// - `reflect.errors.InvalidArgument` if `T` is not a class, if a value
    ///   is not of its field's type, if a field whose type does not accept
    ///   `null` is left out, or if a key names no field of `T`. The message
    ///   names the field and the type it expects. It is also thrown for a
    ///   class that a class literal cannot build either: a reflection kind
    ///   such as `reflect.Class`, or a class holding native state such as
    ///   `baml.fs.File`.
    ///
    /// # Examples
    ///
    /// ```baml
    /// let ticket = reflect.Class.instantiate<Ticket>({ "team": Team.Technical, "urgent": true });
    ///
    /// let type T = ty;  // `ty` is a class held as a `reflect.Type`
    /// let value = reflect.Class.instantiate<T>(fields);
    /// ```
    //baml:mut_vm
    function instantiate<T>(fields: map<string, unknown>) -> T throws root.errors.InvalidArgument {
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
    function metadata(self) -> root.Meta throws never {
        $rust_function
    }
    /// The class's name, rendered like `to_string`: `Runtime`,
    /// `shape.Inner`, `Box<int>`.
    function name(self) -> string throws never {
        self.as_type()._to_string_impl()
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

    /// The value of the enum `T` whose declared name is `name`.
    ///
    /// `name` is the variant's declared name, not its `@alias`. A variant
    /// marked `@skip` is found like any other.
    ///
    /// # Parameters
    ///
    /// - `name`: the declared name of the variant.
    ///
    /// # Returns
    ///
    /// The enum value, equal to writing the variant in code.
    ///
    /// # Throws
    ///
    /// - `reflect.errors.InvalidArgument` if `T` is not an enum, or if `T`
    ///   declares no variant called `name`.
    ///
    /// # Examples
    ///
    /// ```baml
    /// reflect.Enum.value_of<Team>("Technical") == Team.Technical  // true
    /// ```
    //baml:mut_vm
    function value_of<T>(name: string) -> T throws root.errors.InvalidArgument {
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
    function metadata(self) -> root.Meta throws never {
        $rust_function
    }
    /// The enum's name, rendered like `to_string`: `Color`, `shape.Kind`.
    function name(self) -> string throws never {
        self.as_type()._to_string_impl()
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
    function new(types: root.TypeView[]) -> root.Union throws root.errors.CompilationError {
        Union._new(root._as_types(types))
    }

    /// (internal) Internal native backing `new`; every member is a `type`
    /// value.
    //baml:mut_vm
    function _new(types: root.Type[]) -> root.Union throws root.errors.CompilationError {
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
    function new(key: root.TypeView, value: root.TypeView) -> root.Map throws never {
        Map._new(key.as_type(), value.as_type())
    }

    /// (internal) Internal native backing `new`; both types are `type` values.
    //baml:mut_vm
    function _new(key: root.Type, value: root.Type) -> root.Map throws never {
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
