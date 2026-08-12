//baml:mut_vm
function id() -> LocalId throws baml.errors.InvalidArgument {
    $rust_function
}

class LocalId {
    _handle: $rust_type,

    //baml:mut_vm
    function capture(
        self,
        inputs: bool? = null,
        output: bool? = null,
        error: bool? = null,
    ) -> LocalId throws baml.errors.InvalidArgument {
        $rust_function
    }
}
