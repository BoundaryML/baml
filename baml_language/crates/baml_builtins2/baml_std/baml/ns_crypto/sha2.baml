/// SHA-256, the 256-bit member of the SHA-2 family (FIPS 180-4).
///
/// Hashing is incremental. Feed the message in with `update`, in as many pieces
/// as convenient, then read the 32-byte digest with `finish`:
///
/// ```
/// let h = baml.crypto.Sha256.new();
/// h.update("hello".to_utf8());
/// h.update(" world".to_utf8());
/// h.finish().to_hex()  // b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9
/// ```
///
/// A `Sha256` is a handle onto hasher state, not a value. Two references to the
/// same hasher feed the same digest, and `update` from separate `spawn` fibers
/// is serialized rather than racing. Use a fresh `Sha256.new()` per message.
///
/// SHA-256 is a plain hash, not a keyed one. It authenticates nothing on its
/// own: anyone can recompute the digest of a message they can guess. Do not use
/// it to check that a message came from who you think, and do not hash passwords
/// with it.
class Sha256 {
    /// Opaque, runtime-owned hasher state: the SHA-256 compression state and
    /// the buffered tail of the message so far.
    _state: $rust_type,

    /// Creates a hasher with no data fed into it yet. Calling `finish` on it
    /// immediately returns the digest of the empty message.
    //baml:mut_vm
    function new() -> Sha256 throws never {
        $rust_function
    }

    implements Hasher {
        /// Feeds `data` into the hasher. Splitting a message across several
        /// `update` calls gives the same digest as one call with the whole
        /// message.
        //baml:vm
        function update(self, data: uint8array) -> void throws never {
            $rust_function
        }

        /// Returns the 32-byte SHA-256 digest of everything fed in since the
        /// hasher was created or last finished.
        ///
        /// This resets the hasher, so the next `update` starts a new message and
        /// calling `finish` twice in a row returns the digest of the empty
        /// message the second time.
        //baml:vm
        function finish(self) -> uint8array throws never {
            $rust_function
        }
    }
}
