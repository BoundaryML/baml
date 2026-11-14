/// HMAC-SHA256 (RFC 2104): a keyed message authentication code over SHA-256.
///
/// A plain hash proves nothing about origin, since anyone can recompute it. An
/// HMAC tag can only be produced or checked by a holder of the key, which is
/// what a shared-secret handshake such as a webhook signature needs.
///
/// ```
/// let mac = baml.crypto.HmacSha256.new(secret);
/// mac.update(body);
/// baml.crypto.constant_time_eq(mac.finish(), uint8array.from_hex(header_hex))
/// ```
///
/// Compare tags with `constant_time_eq`, never `==`. Like `Sha256`, this is a
/// handle onto mutable state, and `finish` resets it for a new message under
/// the same key.
class HmacSha256 {
    /// Opaque, runtime-owned MAC state. The key cannot be read back or
    /// serialized.
    _state: $rust_type,

    /// Creates a MAC under `key` with no data fed into it yet.
    ///
    /// Args:
    ///   key: Any length. Keys longer than 64 bytes are hashed down first;
    ///     keys shorter than 32 bytes give less than SHA-256's full strength.
    //baml:mut_vm
    function new(key: uint8array) -> HmacSha256 throws never {
        $rust_function
    }

    /// Feeds `data` into the MAC. Chunking does not affect the tag.
    ///
    /// Args:
    ///   data: The next piece of the message.
    //baml:vm
    function update(self, data: uint8array) -> void throws never {
        $rust_function
    }

    /// Finishes the current message and resets for the next one under the
    /// same key.
    ///
    /// Returns:
    ///   The 32-byte tag of everything fed in since creation or the last
    ///   `finish`.
    //baml:vm
    function finish(self) -> uint8array throws never {
        $rust_function
    }
}

/// Compares two byte strings in time that depends only on their length, not on
/// where they first differ. Use it for tags, tokens, and password hashes,
/// where `==` would leak how many leading bytes matched.
///
/// Args:
///   a: One byte string.
///   b: The other.
///
/// Returns:
///   `true` if `a` and `b` are identical. Different lengths return `false`
///   immediately; lengths are public, so that leaks nothing.
//baml:vm
function constant_time_eq(a: uint8array, b: uint8array) -> bool throws never {
    $rust_function
}
