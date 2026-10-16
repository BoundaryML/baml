/// An immutable structural-conformance witness.  Values are created only by
/// `reflect.Interface.implementation<I>()`; each `field` call returns a new witness.
/// A structural witness that a runtime-built class implements an interface.
///
/// Start one with `reflect.Interface.implementation<I>()` and link each
/// interface field to the class field satisfying it with `field`.
class Implementation {
    _handle: $rust_type,

    /// Links `interface_field` to the class field that satisfies it,
    /// defaulting to the field of the same name.
    ///
    /// # Throws
    ///
    /// - `reflect.errors.CompilationError` if the interface declares no such
    ///   field, or the link is not well typed.
    //baml:mut_vm
    function field(
        self,
        interface_field: string,
        class_field: string? = null,
    ) -> Implementation throws root.errors.CompilationError {
        $rust_function
    }
}
