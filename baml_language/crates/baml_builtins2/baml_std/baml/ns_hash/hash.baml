/// An incremental consumer of hash input.
///
/// Implementations may choose their own algorithm. This does not configure
/// the hashing algorithm used by maps.
interface Hasher {
    /// Appends `bytes` to the input stream.
    ///
    /// Writing a stream in chunks must have the same result as writing its
    /// concatenation. `Hash` implementations provide value boundaries.
    function write(self, bytes: uint8array) -> void throws never;

    /// Returns the signed hash of the input so far, without resetting the state.
    ///
    /// Repeated calls without intervening writes return the same value.
    function finish(self) -> int throws never;
}

/// The built-in non-cryptographic hasher.
///
/// New instances start from the same state, so equal hash input produces equal
/// results. The algorithm and resulting integers may change between releases
/// or platforms. Do not persist hashes, use them as identifiers, or use them to
/// authenticate data.
///
/// References share state. Use separate instances for independent hashes.
///
/// ```baml
/// let state = baml.hash.DefaultHasher.new();
/// "hello".hash(state)
/// let result = state.finish();
/// ```
class DefaultHasher {
    _state: $rust_type,

    //baml:mut_vm
    function new() -> DefaultHasher throws never {
        $rust_function
    }

    implements Hasher {
        //baml:vm
        function write(self, bytes: uint8array) -> void throws never {
            $rust_function
        }

        //baml:vm
        function finish(self) -> int throws never {
            $rust_function
        }
    }
}
