/// An interface-kind view of a `type` value. Users never construct this class.
class Type {
    /// The wrapped `type` value. Private by invariant: reflection natives are
    /// its only readers, and each kind method panics if the held type no longer
    /// classifies as this view's kind.
    _ty: reflect.Type,

    implements reflect.TypeView {
        //baml:vm
        //baml:fallible
        function as_type(self) -> reflect.Type throws never {
            $rust_function
        }
    }
    //baml:vm
    //baml:fallible
    function implemented_by(self, other: reflect.Type) -> bool throws never {
        $rust_function
    }
}

/// An immutable structural-conformance witness.  Values are created only by
/// `implementation<I>()`; each `field` call returns a new witness.
class Implementation {
    _handle: $rust_type,

    //baml:mut_vm
    function field(
        self,
        interface_field: string,
        class_field: string? = null,
    ) -> Implementation throws reflect.errors.CompilationError {
        $rust_function
    }
}

/// Start a structural witness for a static interface.
//baml:mut_vm
function implementation<I>() -> Implementation throws reflect.errors.CompilationError {
    $rust_function
}
