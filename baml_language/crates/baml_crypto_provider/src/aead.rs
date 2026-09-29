use std::sync::Arc;

use aes_gcm_siv::aead::{self, Aead, KeyInit, Payload};
use baml_crypto_types::{AeadAlgorithm, AeadCipher, AeadError};

/// Create the requested cipher. A replacement provider may reject any algorithm
/// with `AeadError::Unsupported`; callers must not substitute another algorithm.
pub(crate) fn aead(algorithm: AeadAlgorithm, key: &[u8]) -> Result<Arc<dyn AeadCipher>, AeadError> {
    algorithm.validate_key(key)?;
    match algorithm {
        AeadAlgorithm::Aes128GcmSiv => make::<aes_gcm_siv::Aes128GcmSiv>(algorithm, key),
        AeadAlgorithm::Aes256GcmSiv => make::<aes_gcm_siv::Aes256GcmSiv>(algorithm, key),
        AeadAlgorithm::ChaCha20Poly1305 => {
            make::<chacha20poly1305::ChaCha20Poly1305>(algorithm, key)
        }
        AeadAlgorithm::XChaCha20Poly1305 => {
            make::<chacha20poly1305::XChaCha20Poly1305>(algorithm, key)
        }
    }
}

struct Cipher<C> {
    algorithm: AeadAlgorithm,
    cipher: C,
}

fn make<C: Aead + KeyInit + Send + Sync + 'static>(
    algorithm: AeadAlgorithm,
    key: &[u8],
) -> Result<Arc<dyn AeadCipher>, AeadError> {
    let cipher = C::new_from_slice(key).map_err(|_| {
        AeadError::InvalidArgument(format!("{}: invalid key length", algorithm.name()))
    })?;
    Ok(Arc::new(Cipher { algorithm, cipher }))
}

impl<C: Aead + Send + Sync + 'static> AeadCipher for Cipher<C> {
    fn encrypt(&self, nonce: &[u8], plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>, AeadError> {
        self.algorithm.validate_encrypt(nonce, plaintext, aad)?;
        let nonce = <&aead::Nonce<C>>::try_from(nonce)
            .map_err(|_| AeadError::InvalidArgument("invalid nonce length".to_string()))?;
        self.cipher
            .encrypt(
                nonce,
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .map_err(|_| AeadError::Failed(format!("{}: encryption failed", self.algorithm.name())))
    }

    fn decrypt(&self, nonce: &[u8], ciphertext: &[u8], aad: &[u8]) -> Result<Vec<u8>, AeadError> {
        self.algorithm.validate_decrypt(nonce, ciphertext, aad)?;
        let nonce = <&aead::Nonce<C>>::try_from(nonce)
            .map_err(|_| AeadError::InvalidArgument("invalid nonce length".to_string()))?;
        self.cipher
            .decrypt(
                nonce,
                Payload {
                    msg: ciphertext,
                    aad,
                },
            )
            .map_err(|_| AeadError::AuthenticationFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unhex(value: &str) -> Vec<u8> {
        value
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }

    // Published vectors, also exercised through the language in ns_crypto.
    // These pin the provider interface's nonce/AAD ordering and appended tag.
    #[test]
    fn published_vectors() {
        use AeadAlgorithm::{Aes128GcmSiv, Aes256GcmSiv, ChaCha20Poly1305, XChaCha20Poly1305};
        let message = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";
        let vectors = [
            (
                Aes128GcmSiv,
                "01000000000000000000000000000000",
                "030000000000000000000000",
                unhex("0200000000000000"),
                "01",
                "1e6daba35669f4273b0a1a2560969cdf790d99759abd1508",
            ),
            (
                Aes256GcmSiv,
                "0100000000000000000000000000000000000000000000000000000000000000",
                "030000000000000000000000",
                unhex("0200000000000000"),
                "01",
                "1de22967237a813291213f267e3b452f02d01ae33e4ec854",
            ),
            (
                ChaCha20Poly1305,
                "808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f",
                "070000004041424344454647",
                message.to_vec(),
                "50515253c0c1c2c3c4c5c6c7",
                "d31a8d34648e60db7b86afbc53ef7ec2a4aded51296e08fea9e2b5a736ee62d63dbea45e8ca9671282fafb69da92728b1a71de0a9e060b2905d6a5b67ecd3b3692ddbd7f2d778b8c9803aee328091b58fab324e4fad675945585808b4831d7bc3ff4def08e4b7a9de576d26586cec64b61161ae10b594f09e26a7e902ecbd0600691",
            ),
            (
                XChaCha20Poly1305,
                "808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f",
                "404142434445464748494a4b4c4d4e4f5051525354555657",
                message.to_vec(),
                "50515253c0c1c2c3c4c5c6c7",
                "bd6d179d3e83d43b9576579493c0e939572a1700252bfaccbed2902c21396cbb731c7f1b0b4aa6440bf3a82f4eda7e39ae64c6708c54c216cb96b72e1213b4522f8c9ba40db5d945b11b69b982c1bb9e3f3fac2bc369488f76b2383565d3fff921f9664c97637da9768812f615c68b13b52ec0875924c1c7987947deafd8780acf49",
            ),
        ];
        for (algorithm, key, nonce, plaintext, aad, expected) in vectors {
            let cipher = aead(algorithm, &unhex(key)).unwrap();
            let (nonce, aad, expected) = (unhex(nonce), unhex(aad), unhex(expected));
            assert_eq!(
                cipher.encrypt(&nonce, &plaintext, &aad).unwrap(),
                expected,
                "{algorithm:?}"
            );
            assert_eq!(
                cipher.decrypt(&nonce, &expected, &aad).unwrap(),
                plaintext,
                "{algorithm:?}"
            );
        }
    }

    #[test]
    fn rejects_invalid_inputs_and_authentication_failures() {
        for algorithm in [
            AeadAlgorithm::Aes128GcmSiv,
            AeadAlgorithm::Aes256GcmSiv,
            AeadAlgorithm::ChaCha20Poly1305,
            AeadAlgorithm::XChaCha20Poly1305,
        ] {
            assert!(matches!(
                aead(algorithm, &[]),
                Err(AeadError::InvalidArgument(_))
            ));
            let cipher = aead(algorithm, &vec![0; algorithm.key_len()]).unwrap();
            let nonce = vec![0; algorithm.nonce_len()];
            assert!(matches!(
                cipher.encrypt(&[], b"message", b"aad"),
                Err(AeadError::InvalidArgument(_))
            ));
            assert!(matches!(
                cipher.decrypt(&[], b"", b""),
                Err(AeadError::InvalidArgument(_))
            ));
            assert_eq!(
                cipher.decrypt(&nonce, &[0; 15], b""),
                Err(AeadError::CiphertextTooShort)
            );
            let sealed = cipher.encrypt(&nonce, b"message", b"aad").unwrap();
            assert_eq!(
                cipher.decrypt(&nonce, &sealed, b"other aad"),
                Err(AeadError::AuthenticationFailed)
            );
            let mut wrong_nonce = nonce.clone();
            wrong_nonce[0] ^= 1;
            assert_eq!(
                cipher.decrypt(&wrong_nonce, &sealed, b"aad"),
                Err(AeadError::AuthenticationFailed)
            );
            let wrong_key = aead(algorithm, &vec![1; algorithm.key_len()]).unwrap();
            assert_eq!(
                wrong_key.decrypt(&nonce, &sealed, b"aad"),
                Err(AeadError::AuthenticationFailed)
            );
            let mut corrupted = sealed;
            corrupted[0] ^= 1;
            assert_eq!(
                cipher.decrypt(&nonce, &corrupted, b"aad"),
                Err(AeadError::AuthenticationFailed)
            );
        }
    }
}
