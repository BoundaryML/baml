//! RS256 on `ring`. The `rsa` crate only decodes keys (RUSTSEC-2023-0071).

use std::sync::Arc;

use bex_heap::TlabHolder;
use bex_str::BexStr;
use bex_vm_types::types::Value;
use ring::{rand::SystemRandom, signature};
use rsa::{
    RsaPublicKey,
    pkcs1::DecodeRsaPublicKey,
    pkcs8::{DecodePublicKey, SecretDocument},
    traits::PublicKeyParts,
};

use super::{
    super::{BamlClassCryptoRsaPrivateKey, BamlClassCryptoRsaPublicKey},
    BexVm, PackageBamlImpl, VmRustFnError, copy, invalid_argument, view,
};

const SIGNING_FAILURE: baml_base::LangItem = crate::lang_items::BAML_CRYPTO_SIGNING_FAILURE;

const MAX_PEM_BYTES: usize = 16 * 1024;

#[derive(Clone)]
struct PublicKey {
    n: Vec<u8>,
    e: Vec<u8>,
}

struct PrivateKey {
    pair: signature::RsaKeyPair,
    public: PublicKey,
}

impl bex_vm_types::BexRustData for PublicKey {
    fn measure(&self, meter: &mut bex_vm_types::Meter) {
        meter.bytes(self.n.len() + self.e.len());
    }
}

impl bex_vm_types::BexRustData for PrivateKey {
    fn measure(&self, meter: &mut bex_vm_types::Meter) {
        // Modulus, CRT components, and Montgomery constants: a few modulus lengths.
        let modulus = self.pair.public().modulus_len();
        meter.bytes(modulus * 6 + self.public.n.len() + self.public.e.len());
    }
}

fn invalid_key() -> VmRustFnError {
    invalid_argument("invalid or unsupported RSA key".to_string())
}

impl PublicKey {
    fn from_components(n: &[u8], e: &[u8]) -> Result<Self, VmRustFnError> {
        // Range-check the raw bytes before any big-integer work.
        if !(256..=512).contains(&n.len())
            || n[0] == 0
            || (n.len() == 256 && n[0] < 128)
            || n[n.len() - 1] & 1 == 0
            || e.is_empty()
            || e.len() > 5
            || e[0] == 0
        {
            return Err(invalid_key());
        }
        let exponent = e.iter().fold(0u64, |value, b| (value << 8) | u64::from(*b));
        if !(65537..=(1u64 << 33) - 1).contains(&exponent) || exponent & 1 == 0 {
            return Err(invalid_key());
        }
        Ok(Self {
            n: n.to_vec(),
            e: e.to_vec(),
        })
    }

    fn from_parsed(key: &RsaPublicKey) -> Result<Self, VmRustFnError> {
        Self::from_components(&key.n().to_bytes_be(), &key.e().to_bytes_be())
    }

    fn from_pem(pem: &str) -> Result<Self, VmRustFnError> {
        if pem.len() > MAX_PEM_BYTES {
            return Err(invalid_key());
        }
        let key = RsaPublicKey::from_public_key_pem(pem)
            .or_else(|_| RsaPublicKey::from_pkcs1_pem(pem))
            .map_err(|_| invalid_key())?;
        Self::from_parsed(&key)
    }

    fn verify(&self, message: &[u8], signature: &[u8]) -> bool {
        if signature.len() != self.n.len() {
            return false;
        }
        signature::RsaPublicKeyComponents {
            n: &self.n,
            e: &self.e,
        }
        .verify(&signature::RSA_PKCS1_2048_8192_SHA256, message, signature)
        .is_ok()
    }
}

impl PrivateKey {
    fn from_pem(pem: &str) -> Result<Self, VmRustFnError> {
        if pem.len() > MAX_PEM_BYTES {
            return Err(invalid_key());
        }
        let (label, der) = SecretDocument::from_pem(pem).map_err(|_| invalid_key())?;
        let pair = match label {
            "PRIVATE KEY" => signature::RsaKeyPair::from_pkcs8(der.as_bytes()),
            "RSA PRIVATE KEY" => signature::RsaKeyPair::from_der(der.as_bytes()),
            _ => return Err(invalid_key()),
        }
        .map_err(|_| invalid_key())?;
        let parsed =
            RsaPublicKey::from_pkcs1_der(pair.public().as_ref()).map_err(|_| invalid_key())?;
        let public = PublicKey::from_parsed(&parsed)?;
        Ok(Self { pair, public })
    }

    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, ring::error::Unspecified> {
        let mut signature = vec![0; self.pair.public().modulus_len()];
        self.pair.sign(
            &signature::RSA_PKCS1_SHA256,
            &SystemRandom::new(),
            message,
            &mut signature,
        )?;
        Ok(signature)
    }
}

fn signing_failure(vm: &mut BexVm) -> VmRustFnError {
    let class_ptr = vm.lang_object(SIGNING_FAILURE);
    let message = Value::object(vm.alloc_string("RS256 signing failed".to_string()));
    VmRustFnError::thrown_fresh(Value::object(vm.alloc_instance(class_ptr, vec![message])))
}

#[expect(
    clippy::used_underscore_items,
    reason = "generated opaque field accessor"
)]
impl BamlClassCryptoRsaPrivateKey for PackageBamlImpl {
    fn from_pem(vm: &mut BexVm, pem: &BexStr) -> Result<Value, VmRustFnError> {
        let key = PrivateKey::from_pem(pem.as_str())?;
        Ok(copy::crypto::RsaPrivateKey {
            _key: Arc::new(key),
        }
        .to_value(vm))
    }

    fn sign_rs256(vm: &mut BexVm, key: &Value, message: &[u8]) -> Result<Vec<u8>, VmRustFnError> {
        let result = {
            let key = view::crypto::RsaPrivateKey {
                instance: vm.as_instance(key)?,
            };
            key._key::<PrivateKey>(vm).sign(message)
        };
        result.map_err(|_| signing_failure(vm))
    }

    fn public_key(vm: &mut BexVm, key: &Value) -> Value {
        let public = {
            let key = view::crypto::RsaPrivateKey {
                instance: vm.as_instance(key).expect("typed RSA private key"),
            };
            key._key::<PrivateKey>(vm).public.clone()
        };
        copy::crypto::RsaPublicKey {
            _key: Arc::new(public),
        }
        .to_value(vm)
    }
}

#[expect(
    clippy::used_underscore_items,
    reason = "generated opaque field accessor"
)]
impl BamlClassCryptoRsaPublicKey for PackageBamlImpl {
    fn from_pem(vm: &mut BexVm, pem: &BexStr) -> Result<Value, VmRustFnError> {
        let key = PublicKey::from_pem(pem.as_str())?;
        Ok(copy::crypto::RsaPublicKey {
            _key: Arc::new(key),
        }
        .to_value(vm))
    }

    fn from_components(vm: &mut BexVm, n: &[u8], e: &[u8]) -> Result<Value, VmRustFnError> {
        let key = PublicKey::from_components(n, e)?;
        Ok(copy::crypto::RsaPublicKey {
            _key: Arc::new(key),
        }
        .to_value(vm))
    }

    fn verify_rs256(
        vm: &BexVm,
        key: &view::crypto::RsaPublicKey<'_>,
        message: &[u8],
        signature: &[u8],
    ) -> bool {
        key._key::<PublicKey>(vm).verify(message, signature)
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_PEM_BYTES, PrivateKey, PublicKey};
    use rand_core::SeedableRng;
    use rsa::{
        Pkcs1v15Sign, Pss,
        pkcs1::{EncodeRsaPrivateKey, EncodeRsaPublicKey},
        pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding},
        traits::PublicKeyParts,
    };
    use sha2::Digest;

    const MESSAGE: &[u8] = b"eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiI0MiJ9";

    /// A throwaway key generated per test run; nothing is checked in.
    fn generate(bits: usize) -> rsa::RsaPrivateKey {
        let mut rng = rand_chacha::ChaCha20Rng::seed_from_u64(0x5eed);
        rsa::RsaPrivateKey::new(&mut rng, bits).unwrap()
    }

    fn rsa_crate_signature(key: &rsa::RsaPrivateKey, message: &[u8]) -> Vec<u8> {
        key.sign(
            Pkcs1v15Sign::new::<sha2::Sha256>(),
            &sha2::Sha256::digest(message),
        )
        .unwrap()
    }

    #[test]
    fn signs_and_verifies_across_key_formats_and_backends() {
        let generated = generate(2048);
        let expected = rsa_crate_signature(&generated, MESSAGE);
        let pkcs8 = generated.to_pkcs8_pem(LineEnding::LF).unwrap();
        let pkcs1 = generated.to_pkcs1_pem(LineEnding::LF).unwrap();
        for pem in [pkcs8.as_str(), pkcs1.as_str()] {
            let key = PrivateKey::from_pem(pem).unwrap();
            assert_eq!(key.sign(MESSAGE).unwrap(), expected);
            assert!(key.public.verify(MESSAGE, &expected));
        }
        let public = generated.to_public_key();
        let spki = public.to_public_key_pem(LineEnding::LF).unwrap();
        let pkcs1 = public.to_pkcs1_pem(LineEnding::LF).unwrap();
        for pem in [spki.as_str(), pkcs1.as_str()] {
            assert!(PublicKey::from_pem(pem).unwrap().verify(MESSAGE, &expected));
        }
        let components =
            PublicKey::from_components(&public.n().to_bytes_be(), &public.e().to_bytes_be())
                .unwrap();
        assert!(components.verify(MESSAGE, &expected));
        assert_eq!(components.n, PrivateKey::from_pem(&pkcs8).unwrap().public.n);
    }

    #[test]
    fn rejects_wrong_message_signature_key_and_algorithm() {
        let generated = generate(2048);
        let signature = rsa_crate_signature(&generated, MESSAGE);
        let key = PrivateKey::from_pem(&generated.to_pkcs8_pem(LineEnding::LF).unwrap())
            .unwrap()
            .public;
        assert!(!key.verify(b"tampered", &signature));
        let mut tampered = signature.clone();
        tampered[20] ^= 1;
        let mut rng = rand_chacha::ChaCha20Rng::seed_from_u64(1);
        let pss = generated
            .sign_with_rng(
                &mut rng,
                Pss::new::<sha2::Sha256>(),
                &sha2::Sha256::digest(MESSAGE),
            )
            .unwrap();
        let sha384 = generated
            .sign(
                Pkcs1v15Sign::new::<sha2::Sha384>(),
                &sha2::Sha384::digest(MESSAGE),
            )
            .unwrap();
        for bad in [
            tampered.as_slice(),
            &signature[..255],
            &[0; 257],
            &[],
            &[0; 256],
            &pss,
            &sha384,
        ] {
            assert!(!key.verify(MESSAGE, bad));
        }
        let mut wrong_n = key.n.clone();
        wrong_n[20] ^= 1;
        assert!(
            !PublicKey::from_components(&wrong_n, &key.e)
                .unwrap()
                .verify(MESSAGE, &signature)
        );
    }

    #[test]
    fn rejects_malformed_weak_and_unsupported_pem() {
        let weak = generate(1024);
        let public = generate(2048)
            .to_public_key()
            .to_public_key_pem(LineEnding::LF)
            .unwrap();
        let private = generate(2048).to_pkcs8_pem(LineEnding::LF).unwrap();
        for pem in [
            "",
            "not a PEM",
            public.as_str(),
            &weak.to_pkcs8_pem(LineEnding::LF).unwrap(),
            &"x".repeat(MAX_PEM_BYTES + 1),
            &private.replace("PRIVATE KEY", "ENCRYPTED PRIVATE KEY"),
        ] {
            assert!(PrivateKey::from_pem(pem).is_err());
        }
        for pem in [
            "",
            "not a PEM",
            private.as_str(),
            &weak
                .to_public_key()
                .to_public_key_pem(LineEnding::LF)
                .unwrap(),
            &"x".repeat(MAX_PEM_BYTES + 1),
        ] {
            assert!(PublicKey::from_pem(pem).is_err());
        }
        let Err(error) = PrivateKey::from_pem("secret material") else {
            panic!("invalid key accepted");
        };
        assert!(!format!("{error:?}").contains("secret material"));
    }

    #[test]
    fn rejects_invalid_public_components_before_allocation() {
        let n = [0xff; 256];
        let e = [1, 0, 1];
        let mut even_n = n;
        even_n[255] &= !1;
        let mut small_n = n;
        small_n[0] = 0x7f;
        for bad in [
            &[][..],
            &[1],
            &n[..255],
            &even_n,
            &small_n,
            &[0; 256],
            &[1; 513],
            &[&[0][..], &n].concat(),
        ] {
            assert!(PublicKey::from_components(bad, &e).is_err());
        }
        for bad in [
            &[][..],
            &[0],
            &[3],
            &[1, 0, 0],
            &[0, 1, 0, 1],
            &[2, 0, 0, 0, 1],
            &[1; 6],
        ] {
            assert!(PublicKey::from_components(&n, bad).is_err());
        }
        assert!(PublicKey::from_components(&[0xff; 512], &[1, 255, 255, 255, 255]).is_ok());
    }
}
