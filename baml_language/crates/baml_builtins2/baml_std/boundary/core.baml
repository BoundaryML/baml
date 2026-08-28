/// Mints a fresh boundary id.
///
/// Reads 16 bytes of OS entropy, so it panics with
/// `baml.panics.HostUnavailable` where no entropy source is reachable — the
/// same channel `baml.random.SystemRandom` uses. There is nothing for it to
/// throw.
//baml:mut_vm
//baml:fallible
function id() -> LocalId throws never {
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
