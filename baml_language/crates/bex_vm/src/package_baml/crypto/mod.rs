//! `baml.crypto`: ciphers, digests, MACs, and signatures.
//!
//! Every class keeps its runtime state in an opaque `$rust_type` field, so key
//! material never becomes a BAML value. Digest and MAC state sits behind a
//! mutex because a BAML value is shared immutably; lock helpers recover from
//! poisoning since the primitives themselves never panic.
//!
//! | module   | BAML                                        | backed by                   |
//! |----------|---------------------------------------------|-----------------------------|
//! | [`aead`] | `Aes{128,256}GcmSiv`, `{X}ChaCha20Poly1305` | `aes-gcm-siv`, `chacha20poly1305` |
//! | [`sha2`] | `Sha256`                                    | `sha2`                      |
//! | [`hmac`] | `HmacSha256`, `constant_time_eq`            | `hmac`, `subtle`            |
//! | [`rsa`]  | `RsaPrivateKey`, `RsaPublicKey` (RS256)     | `ring`; `rsa` for parsing   |

mod aead;
mod hmac;
mod rsa;
mod sha2;

use super::{BamlNamespaceCrypto, PackageBamlImpl, copy, view};
use crate::{
    BexVm,
    errors::{VmBamlError, VmRustFnError},
};

fn invalid_argument(message: String) -> VmRustFnError {
    VmRustFnError::BamlError(VmBamlError::InvalidArgument { message })
}

impl BamlNamespaceCrypto for PackageBamlImpl {
    fn constant_time_eq(_: &BexVm, a: &[u8], b: &[u8]) -> bool {
        hmac::constant_time_eq(a, b)
    }
}
