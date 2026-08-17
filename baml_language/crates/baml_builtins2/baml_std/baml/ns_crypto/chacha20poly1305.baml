/// ChaCha20-Poly1305 (RFC 8439): authenticated encryption under a 256-bit key
/// with a 96-bit nonce.
///
/// A stream cipher paired with a one-time authenticator. It runs on ordinary
/// arithmetic rather than table lookups, so it is naturally constant-time and
/// fast in software. Prefer it over the AES ciphers on hardware without AES
/// instructions, and where a peer or format already specifies it (TLS, SSH,
/// WireGuard, `age`).
///
/// ```
/// let rng = baml.random.SystemRandom.get();
/// let key = baml.crypto.ChaCha20Poly1305.random_key(rng);
/// let cipher = baml.crypto.ChaCha20Poly1305.new(key);
///
/// let nonce = rng.random(12);
/// let sealed = cipher.encrypt(nonce, plaintext, aad);
/// let opened = cipher.decrypt(nonce, sealed, aad);
/// ```
///
/// ## Never reuse a nonce
///
/// Unlike [`Aes256GcmSiv`], this construction offers nothing when a
/// `(key, nonce)` pair repeats. Two messages encrypted under the same pair leak
/// the XOR of their plaintexts, and the repetition also exposes the Poly1305
/// key, which lets an attacker forge ciphertexts that authenticate. One repeat
/// is enough to lose both confidentiality and integrity for that key.
///
/// A 96-bit nonce is too small to draw at random for a long-lived key: at
/// random, repeats become likely after roughly 2^48 messages. Derive nonces from
/// a counter you know is unique, or use [`XChaCha20Poly1305`], whose 192-bit
/// nonce is large enough to draw at random.
///
/// The nonce is not part of the ciphertext. Store or transmit it alongside.
class ChaCha20Poly1305 {
    /// Opaque, runtime-owned cipher state.
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
    function new(key: uint8array) -> ChaCha20Poly1305 throws root.errors.InvalidArgument {
        $rust_function
    }

    implements Aead {
        /// Encrypts `plaintext` under `nonce`, authenticating `aad` alongside
        /// it. The ciphertext is 16 bytes longer than `plaintext`, which is the
        /// appended authentication tag.
        ///
        /// `nonce` must never have been used with this key before. See the class
        /// documentation for what a repeat costs.
        ///
        /// ## Throws
        ///
        /// - `baml.errors.InvalidArgument` if `nonce` is not exactly 12 bytes,
        ///   or `plaintext` exceeds the RFC 8439 limit of 2^38 bytes.
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
        ///   or `ciphertext` exceeds the RFC 8439 limit.
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
        /// `ChaCha20Poly1305.new`.
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

/// XChaCha20-Poly1305: [`ChaCha20Poly1305`] with a 192-bit nonce.
///
/// The extended nonce is the whole point. It is hashed down to a per-message
/// subkey before the standard construction runs, so the same 256-bit key,
/// stream cipher, and authenticator do the work, with a nonce large enough to
/// draw uniformly at random. At 192 bits, a repeat is negligible even across an
/// enormous number of messages, which removes the counter bookkeeping that
/// `ChaCha20Poly1305` demands.
///
/// Reach for this whenever nonces come from an RNG rather than from a counter
/// you control:
///
/// ```
/// let rng = baml.random.SystemRandom.get();
/// let key = baml.crypto.XChaCha20Poly1305.random_key(rng);
/// let cipher = baml.crypto.XChaCha20Poly1305.new(key);
///
/// let nonce = rng.random(24);
/// let sealed = cipher.encrypt(nonce, plaintext, aad);
/// let opened = cipher.decrypt(nonce, sealed, aad);
/// ```
///
/// A deliberately reused nonce still costs exactly what it costs for
/// `ChaCha20Poly1305`. The larger nonce makes an accidental collision
/// negligible; it does not make repetition safe.
///
/// The nonce is not part of the ciphertext. Store or transmit it alongside.
class XChaCha20Poly1305 {
    /// Opaque, runtime-owned cipher state.
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
    function new(key: uint8array) -> XChaCha20Poly1305 throws root.errors.InvalidArgument {
        $rust_function
    }

    implements Aead {
        /// Encrypts `plaintext` under `nonce`, authenticating `aad` alongside
        /// it. The ciphertext is 16 bytes longer than `plaintext`, which is the
        /// appended authentication tag.
        ///
        /// ## Throws
        ///
        /// - `baml.errors.InvalidArgument` if `nonce` is not exactly 24 bytes,
        ///   or `plaintext` exceeds the RFC 8439 limit of 2^38 bytes.
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
        /// - `baml.errors.InvalidArgument` if `nonce` is not exactly 24 bytes,
        ///   or `ciphertext` exceeds the RFC 8439 limit.
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
        /// `XChaCha20Poly1305.new`.
        // BUG: see `ChaCha20Poly1305.random_key`. `Self.Key` lowers to an error
        // type at a concrete impl site, so the concrete type is spelled out.
        function random_key(rng: root.random.Rng) -> uint8array throws never {
            rng.random(32)
        }
    }
}
