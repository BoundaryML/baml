/// Authenticated Encryption with Associated Data.
///
/// An `Aead` cipher turns a plaintext into a ciphertext that carries its own
/// authentication tag. `decrypt` rejects the message if the ciphertext, the
/// nonce, or the associated data was altered. Authentication is all or
/// nothing: there is no way to read part of a modified ciphertext.
///
/// `aad` (additional authenticated data) is authenticated but not encrypted.
/// Use it to bind a ciphertext to context the recipient already knows, such as
/// a record id, a protocol version, or a key identifier. A ciphertext lifted
/// into a different context then fails to authenticate. `decrypt` must be given
/// the same `aad` bytes; pass an empty `uint8array` when there is none.
///
/// ## Nonces
///
/// Every implementation fixes a nonce length. A nonce need not be secret, but a
/// `(key, nonce)` pair must not be used for two different plaintexts. Draw each
/// nonce from a counter or from a cryptographically secure `baml.random.Rng`.
/// Implementations document what they still guarantee if a nonce does repeat.
interface Aead {
    /// Encrypts `plaintext` under `nonce`, authenticating `aad` alongside it.
    ///
    /// ## Parameters
    ///
    /// - `nonce`: A nonce of the algorithm's nonce length, not previously used
    ///   with this key.
    /// - `plaintext`: The message to encrypt.
    /// - `aad`: Data authenticated but not encrypted. `decrypt` must be given
    ///   the same bytes.
    ///
    /// ## Returns
    ///
    /// The ciphertext, with the authentication tag appended. The nonce is not
    /// included, so store or transmit it alongside.
    ///
    /// ## Throws
    ///
    /// - `baml.errors.InvalidArgument` if `nonce` is not the algorithm's nonce
    ///   length, or `plaintext` / `aad` exceed the algorithm's size limits.
    function encrypt(
        self,
        nonce: uint8array,
        plaintext: uint8array,
        aad: uint8array,
    ) -> uint8array throws root.errors.InvalidArgument

    /// Authenticates and decrypts `ciphertext`.
    ///
    /// ## Parameters
    ///
    /// - `nonce`: The nonce `encrypt` was given.
    /// - `ciphertext`: The output of `encrypt`, tag included.
    /// - `aad`: The `aad` `encrypt` was given.
    ///
    /// ## Returns
    ///
    /// The original plaintext.
    ///
    /// ## Throws
    ///
    /// - `baml.errors.InvalidArgument` if `nonce` is not the algorithm's nonce
    ///   length, or `ciphertext` / `aad` exceed the algorithm's size limits.
    /// - `DecryptionFailure` if the ciphertext does not authenticate under this
    ///   key, nonce, and `aad`.
    function decrypt(
        self,
        nonce: uint8array,
        ciphertext: uint8array,
        aad: uint8array,
    ) -> uint8array throws root.errors.InvalidArgument | DecryptionFailure
}

/// Implemented by algorithms that can draw a fresh key of their own size.
///
/// The key comes back as raw bytes rather than as a ready-to-use cipher, so it
/// can be stored, wrapped, or transported. Pass it to the algorithm's `new` to
/// get a cipher back.
interface GenerateKey {
    /// The algorithm's key material.
    type Key = uint8array

    /// Draws a fresh key of the algorithm's key length from `rng`.
    ///
    /// A key is only as unpredictable as the generator it came from. Use a
    /// well-seeded cryptographically secure RNG: `baml.random.SystemRandom`, or
    /// a `baml.random.ChaCha20` seeded from it. Never pass
    /// `baml.random.Xoshiro256PlusPlus`, whose output is predictable from
    /// observed draws.
    function random_key(rng: root.random.Rng) -> Self.Key throws never
}
