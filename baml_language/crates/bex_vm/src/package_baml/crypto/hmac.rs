//! `baml.crypto.HmacSha256` and `baml.crypto.constant_time_eq`.

use std::sync::{Arc, Mutex, PoisonError};

use bex_vm_types::types::Value;
use hmac::{Hmac, Mac};

use super::{super::BamlClassCryptoHmacSha256, BexVm, PackageBamlImpl, copy, view};

type HmacSha256 = Hmac<sha2::Sha256>;

/// `keyed` is the state right after keying; `finish` swaps a clone of it back
/// in to reset without holding the raw key. Fixed size, so not metered.
struct HmacState {
    keyed: HmacSha256,
    current: Mutex<HmacSha256>,
}

impl bex_vm_types::BexRustData for HmacState {
    fn measure(&self, _: &mut bex_vm_types::Meter) {}
}

fn state<'v>(mac: &view::crypto::HmacSha256<'_>, vm: &'v BexVm) -> &'v HmacState {
    #[expect(
        clippy::used_underscore_items,
        reason = "the `_state` view accessor is generated from the private BAML field"
    )]
    mac._state::<HmacState>(vm)
}

fn lock_mac<'v>(
    mac: &view::crypto::HmacSha256<'_>,
    vm: &'v BexVm,
) -> std::sync::MutexGuard<'v, HmacSha256> {
    state(mac, vm)
        .current
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

impl BamlClassCryptoHmacSha256 for PackageBamlImpl {
    fn new(vm: &mut BexVm, key: &[u8]) -> Value {
        // `Hmac` accepts any key length, so this cannot fail.
        let keyed = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
        let state: Arc<dyn bex_vm_types::BexRustData> = Arc::new(HmacState {
            current: Mutex::new(keyed.clone()),
            keyed,
        });
        copy::crypto::HmacSha256 { _state: state }.to_value(vm)
    }

    fn update(vm: &BexVm, mac: &view::crypto::HmacSha256<'_>, data: &[u8]) {
        lock_mac(mac, vm).update(data);
    }

    fn finish(vm: &BexVm, mac: &view::crypto::HmacSha256<'_>) -> Vec<u8> {
        let state = state(mac, vm);
        let mut guard = state.current.lock().unwrap_or_else(PoisonError::into_inner);
        let done = std::mem::replace(&mut *guard, state.keyed.clone());
        drop(guard);
        done.finalize().into_bytes().to_vec()
    }
}

/// Constant-time equality; mismatched lengths return early since lengths are public.
pub(super) fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    a.len() == b.len() && bool::from(a.ct_eq(b))
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::*;

    fn tag(key: &[u8], data: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(key).unwrap();
        mac.update(data);
        mac.finalize()
            .into_bytes()
            .iter()
            .fold(String::new(), |mut out, b| {
                let _ = write!(out, "{b:02x}");
                out
            })
    }

    #[test]
    fn rfc4231_vectors() {
        // RFC 4231 test cases 1 and 2.
        assert_eq!(
            tag(&[0x0b; 20], b"Hi There"),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
        assert_eq!(
            tag(b"Jefe", b"what do ya want for nothing?"),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn constant_time_eq_semantics() {
        assert!(constant_time_eq(b"", b""));
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
        assert!(!constant_time_eq(b"", b"a"));
    }
}
