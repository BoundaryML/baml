/// AES-256-GCM-SIV: nonce-misuse-resistant authenticated encryption (RFC 8452)
/// under a 256-bit key.
///
/// The SIV construction derives each message's keystream counter, which is the
/// authentication tag itself, from the plaintext and `aad`. Repeating a
/// `(key, nonce)` pair therefore reveals only whether two messages were
/// identical. It does not leak the authentication key or the XOR of the two
/// plaintexts, both of which plain AES-GCM gives up on nonce reuse. That makes
/// AES-GCM-SIV the safer choice when nonces are drawn at random rather than
/// from a guaranteed-unique counter. It is not a reason to reuse a nonce, only
/// a limit on the damage when one repeats by accident.
///
/// ```
/// let rng = baml.random.SystemRandom.get();
/// let key = baml.crypto.Aes256GcmSiv.random_key(rng);
/// let cipher = baml.crypto.Aes256GcmSiv.new(key);
///
/// let nonce = rng.random(12);
/// let sealed = cipher.encrypt(nonce, plaintext, aad);
/// let opened = cipher.decrypt(nonce, sealed, aad);
/// ```
///
/// The nonce is not part of the ciphertext. Store or transmit it alongside.
class Aes256GcmSiv {
    /// Opaque, runtime-owned cipher state: the expanded AES key schedule.
    ///
    /// The key lives here rather than in a `uint8array` field so that a cipher
    /// carrying a wrong-length key cannot be constructed, and so key material
    /// stays unreachable from BAML. It cannot be read back, rendered by
    /// `string.from`, or serialized by `baml.json.from`.
    _cipher: $rust_type,

    /// Creates a cipher from a 32-byte (256-bit) key.
    ///
    /// ## Throws
    ///
    /// - `baml.errors.InvalidArgument` if `key` is not exactly 32 bytes.
    //baml:mut_vm
    function new(key: uint8array) -> Aes256GcmSiv throws root.errors.InvalidArgument {
        $rust_function
    }

    implements Aead {
        /// Encrypts `plaintext` under `nonce`, authenticating `aad` alongside
        /// it. The ciphertext is 16 bytes longer than `plaintext`, which is the
        /// appended authentication tag.
        ///
        /// ## Throws
        ///
        /// - `baml.errors.InvalidArgument` if `nonce` is not exactly 12 bytes,
        ///   or `plaintext` / `aad` exceed the RFC 8452 limit of 2^36 bytes.
        //baml:vm
        function encrypt(
            self,
            nonce: uint8array,
            plaintext: uint8array,
            aad: uint8array,
        ) -> uint8array throws root.errors.InvalidArgument {
            $rust_function
        }

        /// Authenticates and decrypts `ciphertext`, which must have been
        /// produced by `encrypt` under the same key, `nonce`, and `aad`.
        ///
        /// ## Throws
        ///
        /// - `baml.errors.InvalidArgument` if `nonce` is not exactly 12 bytes,
        ///   or `ciphertext` / `aad` exceed the RFC 8452 limits.
        /// - `DecryptionFailure` if the ciphertext does not authenticate.
        //baml:mut_vm
        function decrypt(
            self,
            nonce: uint8array,
            ciphertext: uint8array,
            aad: uint8array,
        ) -> uint8array throws root.errors.InvalidArgument | DecryptionFailure {
            $rust_function
        }
    }

    implements GenerateKey {
        /// Draws a fresh 32-byte key from `rng`. Pass the result to
        /// `Aes256GcmSiv.new`.
        // BUG: the return type spells out `GenerateKey.Key`'s concrete value
        // instead of `Self.Key`, which lowers to an error type here even though
        // `Self` is the enclosing class and the projection has every input it
        // needs to reduce. Restore `Self.Key` once concrete-site associated
        // projections resolve.
        function random_key(rng: root.random.Rng) -> uint8array throws never {
            rng.random(32)
        }
    }
}

/// AES-128-GCM-SIV: nonce-misuse-resistant authenticated encryption (RFC 8452)
/// under a 128-bit key.
///
/// Identical in construction and guarantees to [`Aes256GcmSiv`], with a shorter
/// key. Prefer `Aes256GcmSiv` unless a 128-bit key is dictated by an existing
/// format or peer.
class Aes128GcmSiv {
    /// Opaque, runtime-owned cipher state: the expanded AES key schedule.
    ///
    /// The key lives here rather than in a `uint8array` field so that a cipher
    /// carrying a wrong-length key cannot be constructed, and so key material
    /// stays unreachable from BAML. It cannot be read back, rendered by
    /// `string.from`, or serialized by `baml.json.from`.
    _cipher: $rust_type,

    /// Creates a cipher from a 16-byte (128-bit) key.
    ///
    /// ## Throws
    ///
    /// - `baml.errors.InvalidArgument` if `key` is not exactly 16 bytes.
    //baml:mut_vm
    function new(key: uint8array) -> Aes128GcmSiv throws root.errors.InvalidArgument {
        $rust_function
    }

    implements Aead {
        /// Encrypts `plaintext` under `nonce`, authenticating `aad` alongside
        /// it. The ciphertext is 16 bytes longer than `plaintext`, which is the
        /// appended authentication tag.
        ///
        /// ## Throws
        ///
        /// - `baml.errors.InvalidArgument` if `nonce` is not exactly 12 bytes,
        ///   or `plaintext` / `aad` exceed the RFC 8452 limit of 2^36 bytes.
        //baml:vm
        function encrypt(
            self,
            nonce: uint8array,
            plaintext: uint8array,
            aad: uint8array,
        ) -> uint8array throws root.errors.InvalidArgument {
            $rust_function
        }

        /// Authenticates and decrypts `ciphertext`, which must have been
        /// produced by `encrypt` under the same key, `nonce`, and `aad`.
        ///
        /// ## Throws
        ///
        /// - `baml.errors.InvalidArgument` if `nonce` is not exactly 12 bytes,
        ///   or `ciphertext` / `aad` exceed the RFC 8452 limits.
        /// - `DecryptionFailure` if the ciphertext does not authenticate.
        //baml:mut_vm
        function decrypt(
            self,
            nonce: uint8array,
            ciphertext: uint8array,
            aad: uint8array,
        ) -> uint8array throws root.errors.InvalidArgument | DecryptionFailure {
            $rust_function
        }
    }

    implements GenerateKey {
        /// Draws a fresh 16-byte key from `rng`. Pass the result to
        /// `Aes128GcmSiv.new`.
        // BUG: see `Aes256GcmSiv.random_key`. `Self.Key` lowers to an error type
        // at a concrete impl site, so the concrete type is spelled out.
        function random_key(rng: root.random.Rng) -> uint8array throws never {
            rng.random(16)
        }
    }
}
