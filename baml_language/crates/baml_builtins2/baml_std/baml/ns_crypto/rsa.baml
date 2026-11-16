/// An RSA private key for RS256 (RSASSA-PKCS1-v1_5 over SHA-256), the
/// algorithm GitHub App authentication requires.
///
/// ```
/// let key = baml.crypto.RsaPrivateKey.from_pem(pem);
/// let sig = key.sign_rs256(signing_input.to_utf8()).to_base64url();
/// ```
///
/// Parsed key material cannot be read back or serialized. The source PEM
/// string is still a secret.
class RsaPrivateKey {
    _key: $rust_type,

    /// Imports an unencrypted PKCS#1 (`RSA PRIVATE KEY`) or PKCS#8
    /// (`PRIVATE KEY`) PEM.
    ///
    /// Args:
    ///   pem: At most 16 KiB. Two-prime keys with a 2048 to 4096-bit modulus
    ///     and an odd exponent from 65537 through 2^33-1.
    ///
    /// Throws:
    ///   baml.errors.InvalidArgument: Malformed, encrypted, or unsupported key.
    //baml:mut_vm
    function from_pem(pem: string) -> RsaPrivateKey throws root.errors.InvalidArgument {
        $rust_function
    }

    /// Signs `message`, hashing it with SHA-256 internally.
    ///
    /// Args:
    ///   message: The raw bytes to sign, not a precomputed hash.
    ///
    /// Returns:
    ///   The raw signature, one modulus length long. Deterministic for a
    ///   given key and message.
    ///
    /// Throws:
    ///   SigningFailure: The backend rejected the key's private components.
    //baml:mut_vm
    function sign_rs256(self, message: uint8array) -> uint8array throws SigningFailure {
        $rust_function
    }

    /// Returns:
    ///   The matching public key.
    //baml:mut_vm
    function public_key(self) -> RsaPublicKey throws never {
        $rust_function
    }
}

/// An RSA public key for RS256 verification.
///
/// A valid signature says only that the key's holder signed these bytes. JWT
/// algorithm, issuer, audience, and expiry checks stay with the caller.
class RsaPublicKey {
    _key: $rust_type,

    /// Imports a PKCS#1 (`RSA PUBLIC KEY`) or SubjectPublicKeyInfo
    /// (`PUBLIC KEY`) PEM.
    ///
    /// Args:
    ///   pem: At most 16 KiB, with the same modulus and exponent limits as
    ///     `RsaPrivateKey.from_pem`.
    ///
    /// Throws:
    ///   baml.errors.InvalidArgument: Malformed or unsupported key.
    //baml:mut_vm
    function from_pem(pem: string) -> RsaPublicKey throws root.errors.InvalidArgument {
        $rust_function
    }

    /// Builds a key from its modulus and exponent, as a JWK carries them.
    ///
    /// Args:
    ///   n: Modulus, unsigned big-endian, no leading zero bytes, odd, 2048 to
    ///     4096 bits. For a JWK, `uint8array.from_base64url(jwk.n)`.
    ///   e: Exponent, same encoding, odd, from 65537 through 2^33-1.
    ///
    /// Throws:
    ///   baml.errors.InvalidArgument: Either component is out of range.
    //baml:mut_vm
    function from_components(
        n: uint8array,
        e: uint8array,
    ) -> RsaPublicKey throws root.errors.InvalidArgument {
        $rust_function
    }

    /// Verifies `signature` over `message`, hashing with SHA-256 internally.
    ///
    /// Args:
    ///   message: The raw signed bytes.
    ///   signature: The raw signature, not base64.
    ///
    /// Returns:
    ///   `false` for any invalid signature, including a wrong length, wrong
    ///   key, or a different algorithm such as PSS or SHA-384.
    //baml:vm
    function verify_rs256(self, message: uint8array, signature: uint8array) -> bool throws never {
        $rust_function
    }
}

/// The backend could not produce an RS256 signature. Carries no key material.
class SigningFailure {
    message: string,
}
