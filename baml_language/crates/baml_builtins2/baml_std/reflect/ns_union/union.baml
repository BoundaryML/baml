/// Constructs the canonical union of `types`.
///
/// # Throws
///
/// - `reflect.errors.CompilationError` if the members cannot form a union.
//baml:mut_vm
function new(types: root.Type[]) -> Type throws root.errors.CompilationError {
    $rust_function
}

/// A union-kind view of a `type` value. Users never construct this class.
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
    /// The union's members, in canonical order.
    //baml:mut_vm
    //baml:fallible
    function member_types(self) -> root.Type[] throws never {
        $rust_function
    }
}
