//! Host-owned opaque values referenced from BAML.
//!
//! A `HostValueArc` is a small Rust-side stub for a value that physically
//! lives in the host language (a Python `function`, Node `JsFunction`, Go
//! `func`, JS `Function`). Rust holds keys + an Arc whose `Drop` notifies
//! the host (via the bridge-installed `HostReleaseFn`) that it can release
//! the underlying user object. The Rust runtime never looks up the key.

use std::sync::Arc;

/// Discriminator for what kind of host value a key refers to.
///
/// Reserved for forward-compatibility: opaque non-callable host values
/// can be added by introducing a new variant without changing the wire
/// shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostValueKind {
    /// A host-language callable (function/closure/method).
    Callable,
}

/// Drop-on-last-clone notification fired to the host language.
pub type HostReleaseFn = extern "C" fn(host_value_key: u64);

/// Inner Arc payload for a host value.
///
/// `Drop` fires `host_release_dispatch::fire(self.key)` so the bridge can
/// remove the underlying host object from its registry.
#[derive(Debug)]
pub struct HostValueArc {
    pub key: u64,
    pub kind: HostValueKind,
}

impl HostValueArc {
    pub fn new(key: u64, kind: HostValueKind) -> Arc<Self> {
        Arc::new(Self { key, kind })
    }
}

impl Drop for HostValueArc {
    fn drop(&mut self) {
        host_release_dispatch::fire(self.key);
    }
}

pub mod host_release_dispatch {
    #[cfg(test)]
    use std::sync::atomic::{AtomicPtr, Ordering};

    use once_cell::sync::OnceCell;

    use super::HostReleaseFn;

    static INSTALLED: OnceCell<HostReleaseFn> = OnceCell::new();

    // For tests we sometimes need to swap implementations. Use an AtomicPtr
    // to fn() so we can override safely without mutex contention.
    #[cfg(test)]
    static TEST_OVERRIDE: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());

    /// Install the bridge's release callback. First call wins; subsequent
    /// calls return `Err`.
    pub fn install(release: HostReleaseFn) -> Result<(), AlreadyInstalled> {
        INSTALLED.set(release).map_err(|_| AlreadyInstalled)
    }

    pub fn fire(key: u64) {
        // Test override path
        #[cfg(test)]
        {
            let p = TEST_OVERRIDE.load(Ordering::Acquire);
            if !p.is_null() {
                // SAFETY: writers of TEST_OVERRIDE put a fn pointer here.
                #[expect(
                    unsafe_code,
                    reason = "test-only AtomicPtr → fn-pointer transmute for the release-dispatch override"
                )]
                let f: HostReleaseFn = unsafe { std::mem::transmute(p) };
                f(key);
                return;
            }
        }
        if let Some(f) = INSTALLED.get() {
            f(key);
        }
        // No installed callback => silently drop. The Rust runtime cannot
        // tell the host anything meaningful before bridge init.
    }

    #[derive(Debug)]
    pub struct AlreadyInstalled;

    #[cfg(test)]
    pub fn install_for_test(release: HostReleaseFn) {
        TEST_OVERRIDE.store(release as *mut (), Ordering::Release);
    }

    #[cfg(test)]
    pub fn clear_test_override() {
        TEST_OVERRIDE.store(std::ptr::null_mut(), Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    static FIRED: Mutex<Vec<u64>> = Mutex::new(Vec::new());

    extern "C" fn record_release(key: u64) {
        FIRED.lock().unwrap().push(key);
    }

    fn reset() {
        FIRED.lock().unwrap().clear();
        host_release_dispatch::clear_test_override();
        host_release_dispatch::install_for_test(record_release);
    }

    #[test]
    fn drop_fires_release_once_at_last_clone() {
        reset();
        let arc1 = HostValueArc::new(42, HostValueKind::Callable);
        let arc2 = arc1.clone();
        drop(arc1);
        assert!(
            FIRED.lock().unwrap().is_empty(),
            "clone alive => no release"
        );
        drop(arc2);
        assert_eq!(FIRED.lock().unwrap().as_slice(), &[42]);
    }

    #[test]
    fn no_release_when_not_installed() {
        // intentionally do not call reset; clear test override first.
        host_release_dispatch::clear_test_override();
        let _ = HostValueArc::new(7, HostValueKind::Callable);
        // last drop happens at end of statement — must not panic with
        // no callback installed.
    }
}
