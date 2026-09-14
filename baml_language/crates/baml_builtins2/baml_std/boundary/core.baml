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

/// A boundary id minted by `boundary.id()`, identifying one traced unit of
/// work.
class LocalId {
    _handle: $rust_type,

    /// Returns this id with capture flags adjusted; a null argument leaves
    /// that flag as it was.
    ///
    /// # Parameters
    ///
    /// - `inputs`: record the traced call's arguments.
    /// - `output`: record its return value.
    /// - `error`: record the error it throws.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if the flags are not a valid combination.
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
