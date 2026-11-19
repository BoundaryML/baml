//! Host-owned opaque values referenced from BAML.
//!
//! A `HostValueArc` is a small Rust-side stub for a value that physically
//! lives in the host language (a Python `function`, Node `JsFunction`, Go
//! `func`, JS `Function`). Rust holds keys + an Arc whose `Drop` notifies
//! the host (via the bridge-installed `HostReleaseFn`) that it can release
//! the underlying user object. The Rust runtime never looks up the key.
//!
//! The boundary has two reference tables that are duals of each other. The
//! engine's handle table holds engine objects under keys *the host holds*; the
//! host's registry holds host objects under keys *the engine holds*, and this
//! module is the engine's side of that second table. A [`HostKey`] is never
//! compared with an engine table key: the two are different types.

use std::{
    any::Any,
    collections::HashMap,
    num::NonZeroU64,
    sync::{Arc, Mutex, PoisonError, Weak},
};

use once_cell::sync::Lazy;

/// Discriminator for what kind of host value a key refers to.
///
/// Opaque non-callable host values are distinguished from callables by
/// their variant; the wire shape (`BamlHandle { key, handle_type }`) is
/// shared, with `handle_type` carrying the discriminant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostValueKind {
    /// A host-language callable (function/closure/method).
    Callable,
    /// An arbitrary host-language value with no BAML representation, referenced
    /// by key and round-tripped by identity. Surfaces in BAML as a
    /// `baml.host.Value`. A native host exception is one
    /// consumer: the bridge registers it as an opaque value and wraps the
    /// handle in `baml.errors.HostCallable`.
    Opaque,
}

/// The engine's reference to a host object: the key the host's registry
/// minted for it. Never 0, which every host reserves for "no value".
///
/// A `HostKey` is not an engine handle-table key and cannot be passed where
/// one is expected; the one place a bare `u64` becomes either is the wire
/// decoder, which decides by the handle's tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HostKey(NonZeroU64);

impl HostKey {
    /// A key as a host minted it; `None` for the reserved 0.
    pub const fn new(key: u64) -> Option<Self> {
        match NonZeroU64::new(key) {
            Some(key) => Some(Self(key)),
            None => None,
        }
    }

    /// The key as the C ABI and the wire carry it.
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl std::fmt::Display for HostKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// The engine giving a host back `count` of the references it holds to the
/// host value `host_value_key`. A host counts the references it hands out per
/// key (one each time the key crosses into the engine) and drops its object
/// when the count reaches zero.
pub type HostReleaseFn = extern "C" fn(host_value_key: u64, count: u64);

/// Inner Arc payload for a host value.
///
/// Every crossing of a host key into the engine transfers one host reference,
/// which the engine's one `Arc` for that key holds (`held`). `Drop` gives them
/// all back (`host_release_dispatch::fire(self.key, held)`).
///
/// `PartialEq` compares by `(key, kind)`: the engine's reference index
/// guarantees one live `Arc<HostValueArc>` per key (see
/// [`Self::try_intern`]), so two `Arc`s with the same key always refer to the
/// same host object.
#[derive(Debug)]
pub struct HostValueArc {
    pub key: HostKey,
    pub kind: HostValueKind,
    annotation: Option<HostAnnotation>,
    /// The host references this identity holds: one per crossing of `key`
    /// into the engine that it took in.
    held: std::sync::atomic::AtomicU64,
}

#[derive(Clone)]
struct HostAnnotation(Arc<dyn Any + Send + Sync>);
impl std::fmt::Debug for HostAnnotation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HostAnnotation(..)")
    }
}
impl PartialEq for HostValueArc {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.kind == other.kind
    }
}
impl Eq for HostValueArc {}

// Registration precedes inbound decoding. The reference index transfers this
// owned, immutable declaration into the value; it contains no invocation or
// executor.
static PENDING_ANNOTATIONS: Lazy<Mutex<HashMap<HostKey, HostAnnotation>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// The engine's index of the host references it holds: each host-value key
/// to a `Weak` handle to the single live `Arc<HostValueArc>` for that key.
///
/// The wire carries only the bare key, so every inbound decode of the same
/// key must yield clones of the *same* `Arc` — otherwise two independent
/// `Arc`s with independent refcounts would each fire
/// `host_release_dispatch::fire(key)` on their own last drop, and the first
/// one to drop would tear the host registry entry out from under the other
/// still-live handle. The index makes `key` a stable identity: one key ⇒ one
/// live `Arc`, so release fires exactly once across the whole process. An
/// entry is `Weak` because it lives exactly as long as the engine holds
/// references, the dual of a handle-table row living while the host does.
static HOST_REFERENCES: Lazy<Mutex<HashMap<HostKey, Weak<HostValueArc>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

impl HostValueArc {
    pub fn register_annotation(key: HostKey, annotation: Arc<dyn Any + Send + Sync>) {
        let replaced = PENDING_ANNOTATIONS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key, HostAnnotation(annotation));
        drop(replaced);
    }

    pub fn discard_annotation(key: HostKey) {
        let removed = PENDING_ANNOTATIONS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&key);
        drop(removed);
    }

    pub fn annotation<T: Any>(&self) -> Option<&T> {
        self.annotation.as_ref()?.0.downcast_ref()
    }

    /// Construct a fresh, *un-indexed* `Arc<HostValueArc>`.
    ///
    /// This bypasses the reference index, so the returned `Arc` has an
    /// independent identity. It is intended for direct construction (e.g.
    /// engine-side values that never round-trip through the FFI wire) and for
    /// unit tests. Inbound FFI decode must use [`HostValueArc::try_intern`] so
    /// that re-decoding the same wire key reuses one `Arc`.
    pub fn new(key: HostKey, kind: HostValueKind) -> Arc<Self> {
        Arc::new(Self {
            key,
            kind,
            annotation: None,
            held: std::sync::atomic::AtomicU64::new(1),
        })
    }

    /// The single live `Arc<HostValueArc>` for `key`, created if none exists,
    /// taking in the one host reference this crossing of `key` transfers.
    ///
    /// This is the identity-stable constructor used at the FFI decode
    /// boundary: repeated decodes of the same wire `key` all yield clones of
    /// one `Arc`, which holds every reference they transferred and gives them
    /// back when the last clone (across the entire process) drops — even when
    /// BAML returns a host callable to the host and it is later passed back
    /// in.
    ///
    /// A key's kind is fixed for its lifetime. When `key` is live under
    /// another kind the result is `Err` with that live `Arc`, so the decoder
    /// can refuse the host's handle instead of minting a second identity for
    /// one key: the kind comes from a host, and a disagreement there is a
    /// refusal of the host's value, not a bug in the engine.
    pub fn try_intern(key: HostKey, kind: HostValueKind) -> Result<Arc<Self>, Arc<Self>> {
        let mut map = HOST_REFERENCES
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(existing) = map.get(&key).and_then(Weak::upgrade) {
            return if existing.kind == kind {
                existing
                    .held
                    .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                Ok(existing)
            } else {
                Err(existing)
            };
        }
        // No live entry (absent, or a dangling `Weak` whose `Arc` already
        // dropped). Create a fresh identity and record a `Weak` to it.
        let annotation = PENDING_ANNOTATIONS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&key);
        let arc = Arc::new(Self {
            key,
            kind,
            annotation,
            held: std::sync::atomic::AtomicU64::new(1),
        });
        map.insert(key, Arc::downgrade(&arc));
        Ok(arc)
    }
}

impl Drop for HostValueArc {
    fn drop(&mut self) {
        // Remove our (now-dangling) index entry before firing release, and
        // never hold the index lock across `fire` — `fire` runs host code
        // (e.g. acquiring the Python GIL) and could re-enter the index.
        {
            let mut map = HOST_REFERENCES
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            // Only remove if the stored `Weak` is *ours* — i.e. it is dead. A
            // concurrent `try_intern(key)` racing this `Drop` may have found
            // our dead `Weak`, replaced it with a fresh live entry, and we must
            // not evict that newer identity. `strong_count() == 0`
            // distinguishes our dead allocation from a newer live one without
            // creating a temporary `Arc`: upgrading here could make this
            // thread the last owner of the newer identity, whose `Drop` would
            // then re-enter this non-reentrant lock. (`self`'s strong count is
            // already 0 here, so our own `Weak` is dead.)
            if map.get(&self.key).is_some_and(|w| w.strong_count() == 0) {
                map.remove(&self.key);
            }
        }
        host_release_dispatch::fire(self.key, *self.held.get_mut());
    }
}

pub mod host_release_dispatch {
    #[cfg(test)]
    use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
    use std::sync::{Mutex, PoisonError};

    use once_cell::sync::{Lazy, OnceCell};

    use super::{HostKey, HostReleaseFn};

    static INSTALLED: OnceCell<HostReleaseFn> = OnceCell::new();

    // When the test override is installed, `fire` fires it inline by default so
    // the original drop-time-release tests stay simple. The deferred-release
    // and resurrection tests flip this to exercise the production enqueue/drain
    // path through the override.
    #[cfg(test)]
    static TEST_DEFER: AtomicBool = AtomicBool::new(false);

    /// Process-global queue of host references the engine has given back and
    /// the host has not yet been told of: a key and how many.
    ///
    /// [`fire`] (called from `HostValueArc::drop`) only enqueues here; it never
    /// invokes the installed [`HostReleaseFn`]. The host callback runs
    /// arbitrary host code (e.g. Python `Python::attach` acquires the GIL), and
    /// `Drop` can run inside the stop-the-world GC window while all heap
    /// permits are parked — firing host code there is an AB-BA deadlock risk.
    /// The engine calls [`drain`] at safepoints *outside* that window to flush
    /// these.
    static PENDING_RELEASES: Lazy<Mutex<Vec<(HostKey, u64)>>> =
        Lazy::new(|| Mutex::new(Vec::new()));

    // For tests we sometimes need to swap implementations. Use an AtomicPtr
    // to fn() so we can override safely without mutex contention.
    #[cfg(test)]
    static TEST_OVERRIDE: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());

    /// Install the bridge's release callback. First call wins; subsequent
    /// calls return `Err`.
    pub fn install(release: HostReleaseFn) -> Result<(), AlreadyInstalled> {
        INSTALLED.set(release).map_err(|_| AlreadyInstalled)
    }

    /// Give the host back `count` references to `key`: the engine no longer
    /// holds them (an identity dropped, or a refused request never took them
    /// in).
    ///
    /// This **enqueues** onto the pending-release queue instead of invoking
    /// the host callback inline — it may be called from a `Drop` running
    /// inside the GC stop-the-world window. The engine flushes the queue via
    /// [`drain`] at a safepoint where no heap permit is parked.
    pub fn fire(key: HostKey, count: u64) {
        if count == 0 {
            return;
        }
        // Test override path: fire inline (default) so the original
        // drop-time-release tests stay simple — the override never runs inside
        // a real GC window. Tests that need the production enqueue/drain
        // behavior flip `TEST_DEFER` to fall through to the queue below.
        #[cfg(test)]
        {
            let p = TEST_OVERRIDE.load(Ordering::Acquire);
            if !p.is_null() && !TEST_DEFER.load(Ordering::Acquire) {
                // SAFETY: writers of TEST_OVERRIDE put a fn pointer here.
                #[expect(
                    unsafe_code,
                    reason = "test-only AtomicPtr → fn-pointer transmute for the release-dispatch override"
                )]
                let f: HostReleaseFn = unsafe { std::mem::transmute(p) };
                f(key.get(), count);
                return;
            }
        }
        PENDING_RELEASES
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((key, count));
    }

    /// Tell the host of every reference the engine has given back.
    ///
    /// Call this only at engine safepoints where **no heap permit is parked**
    /// and no engine lock (`FutureManager`, the reference index) is held — the
    /// host callback runs arbitrary host code.
    ///
    /// Each entry gives back exactly the references one identity took in, so
    /// no entry is ever skipped: a key the host passed in again after its old
    /// identity dropped is a new identity holding the new crossing's
    /// reference, and the host's count stays above zero for it. Repeated
    /// entries for one key are summed into one call.
    pub fn drain() {
        let pending: Vec<(HostKey, u64)> = {
            let mut queue = PENDING_RELEASES
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            std::mem::take(&mut *queue)
        };
        if pending.is_empty() {
            return;
        }
        let mut to_release: Vec<(HostKey, u64)> = Vec::with_capacity(pending.len());
        for (key, count) in pending {
            match to_release.iter_mut().find(|(seen, _)| *seen == key) {
                Some((_, total)) => *total += count,
                None => to_release.push((key, count)),
            }
        }

        // Fire with no locks held — the host callback runs arbitrary code.
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
                for (key, count) in to_release {
                    f(key.get(), count);
                }
                return;
            }
        }
        if let Some(f) = INSTALLED.get() {
            for (key, count) in to_release {
                f(key.get(), count);
            }
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
        TEST_DEFER.store(false, Ordering::Release);
    }

    /// Make the test override go through the production enqueue/drain path
    /// instead of firing inline at drop. Test-only.
    #[cfg(test)]
    pub fn set_test_defer(defer: bool) {
        TEST_DEFER.store(defer, Ordering::Release);
    }

    /// Discard any queued pending releases without firing. Test-only helper to
    /// isolate the deferred-release queue between cases.
    #[cfg(test)]
    pub fn clear_pending_for_test() {
        PENDING_RELEASES
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, MutexGuard};

    use super::*;

    static FIRED: Mutex<Vec<(u64, u64)>> = Mutex::new(Vec::new());

    // `FIRED`, the release-dispatch test override, and the process-global
    // `HOST_REFERENCES` are all shared mutable state. Tests run in parallel
    // threads within one process, so serialize the ones that touch this state.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    extern "C" fn record_release(key: u64, count: u64) {
        FIRED.lock().unwrap().push((key, count));
    }

    fn key(key: u64) -> HostKey {
        HostKey::new(key).expect("test keys are nonzero")
    }

    /// The decoder's use of [`HostValueArc::try_intern`] for a key whose kind
    /// agrees: the live `Arc`, or a fresh one.
    fn intern(key: u64, kind: HostValueKind) -> Arc<HostValueArc> {
        HostValueArc::try_intern(self::key(key), kind).unwrap_or_else(|existing| existing)
    }

    /// Acquire the test serialization lock and reset the recorded-release log
    /// + install the recording release callback.
    fn lock_and_reset() -> MutexGuard<'static, ()> {
        let guard = TEST_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        FIRED.lock().unwrap().clear();
        // `clear_test_override` also resets `TEST_DEFER` to inline firing.
        host_release_dispatch::clear_test_override();
        host_release_dispatch::clear_pending_for_test();
        host_release_dispatch::install_for_test(record_release);
        guard
    }

    #[test]
    fn zero_is_not_a_host_key() {
        assert!(HostKey::new(0).is_none());
        assert_eq!(key(7).get(), 7);
        assert_eq!(key(7).to_string(), "7");
    }

    #[test]
    fn drop_fires_release_once_at_last_clone() {
        let _guard = lock_and_reset();
        let arc1 = HostValueArc::new(key(42), HostValueKind::Callable);
        let arc2 = arc1.clone();
        drop(arc1);
        assert!(
            FIRED.lock().unwrap().is_empty(),
            "clone alive => no release"
        );
        drop(arc2);
        assert_eq!(FIRED.lock().unwrap().as_slice(), &[(42, 1)]);
    }

    #[test]
    fn no_release_when_not_installed() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
        // intentionally do not install the recorder; clear override first.
        host_release_dispatch::clear_test_override();
        let _ = HostValueArc::new(key(7), HostValueKind::Callable);
        // last drop happens at end of statement — must not panic with
        // no callback installed.
    }

    #[test]
    fn intern_same_key_shares_one_arc_and_releases_once() {
        let _guard = lock_and_reset();
        let arc1 = intern(100, HostValueKind::Callable);
        let arc2 = intern(100, HostValueKind::Callable);
        // Two interns of one key must hand out the *same* allocation.
        assert!(
            Arc::ptr_eq(&arc1, &arc2),
            "interning K twice must yield the same Arc identity"
        );

        drop(arc1);
        assert!(
            FIRED.lock().unwrap().is_empty(),
            "a live clone remains => release must not fire yet"
        );

        drop(arc2);
        assert_eq!(
            FIRED.lock().unwrap().as_slice(),
            &[(100, 2)],
            "the last drop gives back both crossings' references at once"
        );
        // The dead entry must have been cleaned out of the index.
        assert!(
            !HOST_REFERENCES.lock().unwrap().contains_key(&key(100)),
            "index entry must be removed once the last Arc drops"
        );
    }

    #[test]
    fn intern_after_release_creates_fresh_identity() {
        let _guard = lock_and_reset();
        let arc1 = intern(101, HostValueKind::Callable);
        drop(arc1);
        assert_eq!(
            FIRED.lock().unwrap().as_slice(),
            &[(101, 1)],
            "release fires when the only Arc drops"
        );

        // Re-interning the same key after release yields a brand-new Arc and a
        // fresh index entry (the old dead Weak was cleaned up).
        let arc2 = intern(101, HostValueKind::Callable);
        drop(arc2);
        assert_eq!(
            FIRED.lock().unwrap().as_slice(),
            &[(101, 1), (101, 1)],
            "the fresh identity gives back its own reference on drop"
        );
    }

    #[test]
    fn round_trip_decode_keeps_one_identity_until_true_last_drop() {
        // Simulates: BAML returns a host callable, the host passes it back in.
        // The first inbound decode produces `arc1`; the second decode (a
        // "pass-back") produces `arc2`. Both must share one identity so that
        // dropping a transient clone does NOT prematurely release the still-
        // live handle. Release fires exactly once, at the true last drop.
        let _guard = lock_and_reset();

        // First decode of the key.
        let arc1 = intern(102, HostValueKind::Callable);
        // A transient clone of arc1 (e.g. it is encoded back out to the host).
        let transient = Arc::clone(&arc1);

        // Second decode of the same key (the host passes it back in) while
        // arc1 is still live.
        let arc2 = intern(102, HostValueKind::Callable);
        assert!(
            Arc::ptr_eq(&arc1, &arc2),
            "re-decode of a live key must reuse the same Arc"
        );

        // The transient clone goes away first.
        drop(transient);
        assert!(
            FIRED.lock().unwrap().is_empty(),
            "dropping a transient clone must not fire release while live"
        );

        drop(arc1);
        assert!(
            FIRED.lock().unwrap().is_empty(),
            "arc2 still alive => no release"
        );

        drop(arc2);
        assert_eq!(
            FIRED.lock().unwrap().as_slice(),
            &[(102, 2)],
            "the last drop gives back both decodes' references"
        );
    }

    /// With deferred dispatch, dropping the last `Arc` must enqueue the key but
    /// NOT fire the release callback inline. Only an explicit `drain` fires it,
    /// and exactly once.
    #[test]
    fn drop_enqueues_and_only_drain_fires() {
        let _guard = lock_and_reset();
        // Route the test override through the production enqueue/drain path.
        host_release_dispatch::set_test_defer(true);

        let arc = intern(200, HostValueKind::Callable);
        drop(arc);
        assert!(
            FIRED.lock().unwrap().is_empty(),
            "last-Arc drop must enqueue, not fire inline"
        );

        host_release_dispatch::drain();
        assert_eq!(
            FIRED.lock().unwrap().as_slice(),
            &[(200, 1)],
            "drain fires the queued release exactly once"
        );

        // A second drain has nothing left to do.
        host_release_dispatch::drain();
        assert_eq!(
            FIRED.lock().unwrap().as_slice(),
            &[(200, 1)],
            "an empty queue must not re-fire"
        );
    }

    /// A key the host passes in again after its identity dropped (but before
    /// the drain) is a new identity holding the new crossing's reference: the
    /// drain still gives back the old identity's, and the host's count stays
    /// above zero for the new one.
    #[test]
    fn a_re_passed_key_gives_back_each_identitys_references() {
        let _guard = lock_and_reset();
        host_release_dispatch::set_test_defer(true);

        let arc1 = intern(201, HostValueKind::Callable);
        drop(arc1);
        let arc2 = intern(201, HostValueKind::Callable);
        host_release_dispatch::drain();
        assert_eq!(
            FIRED.lock().unwrap().as_slice(),
            &[(201, 1)],
            "the dropped identity's reference is given back; the live one keeps its own"
        );

        drop(arc2);
        host_release_dispatch::drain();
        assert_eq!(
            FIRED.lock().unwrap().as_slice(),
            &[(201, 1), (201, 1)],
            "the second identity gives back its own reference"
        );
    }

    #[test]
    fn declaration_transfers_to_interned_identity_and_rollback_releases_it() {
        let _guard = lock_and_reset();
        let marker = Arc::new(String::from("declaration"));
        HostValueArc::register_annotation(key(203), marker.clone());
        assert_eq!(Arc::strong_count(&marker), 2);
        let value = intern(203, HostValueKind::Callable);
        assert_eq!(value.annotation::<String>().unwrap(), "declaration");
        let alias = intern(203, HostValueKind::Callable);
        assert!(Arc::ptr_eq(&value, &alias));
        HostValueArc::discard_annotation(key(203));
        drop(value);
        assert_eq!(Arc::strong_count(&marker), 2);
        drop(alias);
        assert_eq!(Arc::strong_count(&marker), 1);
        HostValueArc::register_annotation(key(204), marker.clone());
        HostValueArc::discard_annotation(key(204));
        assert_eq!(Arc::strong_count(&marker), 1);
        assert!(
            intern(204, HostValueKind::Callable)
                .annotation::<String>()
                .is_none()
        );
    }

    /// A key given back more than once before a drain (drop → re-intern →
    /// drop) is told to the host once, with the references summed.
    #[test]
    fn drain_sums_repeated_releases_of_one_key() {
        let _guard = lock_and_reset();
        host_release_dispatch::set_test_defer(true);

        let arc1 = intern(202, HostValueKind::Callable);
        drop(arc1);
        let arc2 = intern(202, HostValueKind::Callable);
        drop(arc2);
        assert!(
            FIRED.lock().unwrap().is_empty(),
            "deferred dispatch only enqueues at drop"
        );

        host_release_dispatch::drain();
        assert_eq!(FIRED.lock().unwrap().as_slice(), &[(202, 2)]);
    }

    /// A count of zero gives back nothing and tells the host nothing.
    #[test]
    fn giving_back_no_references_tells_the_host_nothing() {
        let _guard = lock_and_reset();
        host_release_dispatch::fire(key(206), 0);
        host_release_dispatch::drain();
        assert!(FIRED.lock().unwrap().is_empty());
    }

    /// `HostValueKind::Opaque` (used for opaque host-value round-trip, e.g. the
    /// host-callable error wrapper) must follow the same intern/refcount/release
    /// contract as `Callable`.
    #[test]
    fn opaque_kind_release_fires_on_last_drop() {
        let _guard = lock_and_reset();
        let arc = HostValueArc::new(key(200), HostValueKind::Opaque);
        drop(arc);
        assert_eq!(
            FIRED.lock().unwrap().as_slice(),
            &[(200, 1)],
            "Opaque-kind release must fire just like Callable"
        );
    }

    /// A key's kind is fixed for its lifetime: a second kind for a live key
    /// is refused with the live identity, never minted beside it, and the
    /// refusal creates nothing to release.
    #[test]
    fn a_live_key_under_another_kind_is_refused_not_aliased() {
        let _guard = lock_and_reset();
        let callable = intern(205, HostValueKind::Callable);
        let refused = HostValueArc::try_intern(key(205), HostValueKind::Opaque)
            .expect_err("a live key keeps its kind");
        assert!(
            Arc::ptr_eq(&callable, &refused),
            "the refusal names the live identity"
        );
        assert_eq!(refused.kind, HostValueKind::Callable);
        drop(refused);
        drop(callable);
        assert_eq!(
            FIRED.lock().unwrap().as_slice(),
            &[(205, 1)],
            "the refusal took in nothing, so only the live crossing is given back"
        );
        // Once dead, the key may be minted again under either kind.
        let opaque = intern(205, HostValueKind::Opaque);
        assert_eq!(opaque.kind, HostValueKind::Opaque);
    }
}
