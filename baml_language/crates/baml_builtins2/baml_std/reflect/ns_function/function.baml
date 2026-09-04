/// One parameter of a function type: its name where the signature gives
/// one, its declared type, and whether it may be omitted.
class Parameter {
    name: string?,
    type: reflect.Type,
    optional: bool,
}

/// A function-kind view of a `type` value. Throws are intentionally not part
/// of the function reflection surface.
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
    //baml:mut_vm
    //baml:fallible
    /// The function's parameters, in declaration order.
    function params(self) -> Parameter[] throws never {
        $rust_function
    }
    //baml:mut_vm
    //baml:fallible
    /// The function's return type.
    function return_type(self) -> reflect.Type throws never {
        $rust_function
    }
}
