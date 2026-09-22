/// Constructs the literal type matching exactly a string, int, bigint, or bool value.
//baml:mut_vm
function new(value: string | int | bigint | bool) -> Type throws never {
    $rust_function
}

/// A literal-kind view of a `type` value. Users never construct this class.
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

    /// Returns the string, int, bigint, bool, or enum variant represented by this literal type.
    /// The return type is unknown because enum variants can belong to any enum.
    /// This API cannot return class values today.
    //baml:mut_vm
    //baml:fallible
    function value(self) -> unknown throws never {
        $rust_function
    }
}
