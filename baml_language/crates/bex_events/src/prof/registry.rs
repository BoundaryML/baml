//! The ring registry and per-thread ring map (plan §3.4, design D5b).
//!
//! The registry is an append-only, push-only Treiber list: rings are
//! CAS-appended once and never removed (no pop ⇒ no ABA; no reclamation ⇒ no
//! epochs). Thread death *orphans* a ring; the consumer drains it to empty
//! and *pools* it; a new `(engine, os-thread)` pair *claims* a pooled ring by
//! CAS before allocating fresh. Registry size is therefore bounded by the
//! peak number of concurrent `(engine, os-thread)` pairs, not by churn —
//! tokio blocking-pool threads die after their idle timeout, so the
//! orphan/pool/claim path is routine, not exotic.
#![allow(unsafe_code)]
// On wasm32 there is no background consumer thread, but the cooperative drain
// uses the same registry sweep path when an adapter opts into profiling.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use std::ptr::null_mut;

use crate::prof::{
    ring::{Ring, RingCtx, RingHandle, RingState},
    sync::{AtomicPtr, Ordering},
};

struct RegNode {
    /// Conceptually `&'static Ring`; kept raw so tests can reclaim with full
    /// provenance. Production never frees it (invariant 7).
    ring: *mut Ring,
    next: AtomicPtr<RegNode>,
}

/// Append-only ring registry. One per process in production
/// ([`global_registry`]); tests build their own.
pub(crate) struct Registry {
    head: AtomicPtr<RegNode>,
}

impl Registry {
    #[cfg(not(baml_loom))]
    pub(crate) const fn new() -> Self {
        Self {
            head: AtomicPtr::new(null_mut()),
        }
    }

    #[cfg(baml_loom)]
    pub(crate) fn new() -> Self {
        Self {
            head: AtomicPtr::new(null_mut()),
        }
    }

    /// Producer-side acquisition: claim a pooled ring, or allocate and
    /// register a fresh one. Runs once per `(engine, os-thread)` lifetime —
    /// the O(rings) scan is irrelevant next to that.
    ///
    /// The returned handle is bound to the calling thread (`!Send`), which is
    /// what upholds the unique-producer contract.
    pub(crate) fn acquire(
        &self,
        ctx: &'static RingCtx,
        seg_bytes: usize,
        freelist_cap: usize,
        engine_id: u64,
    ) -> RingHandle {
        let mut node = self.head.load(Ordering::Acquire);
        while !node.is_null() {
            let n = unsafe { &*node };
            let ring = unsafe { &*n.ring };
            if ring.try_claim(engine_id) {
                // SAFETY: the CAS made this thread the unique producer.
                return unsafe { RingHandle::new(ring) };
            }
            node = n.next.load(Ordering::Acquire);
        }
        // Keep the raw pointer (not a ref-derived copy) in the node so the
        // test-only Registry::drop deallocates with original provenance.
        let ring_ptr = Ring::alloc(ctx, seg_bytes, freelist_cap, engine_id);
        self.push(ring_ptr);
        // SAFETY: a freshly allocated ring is Active and owned by its
        // creating thread.
        unsafe { RingHandle::new(&*ring_ptr) }
    }

    fn push(&self, ring: *mut Ring) {
        let node = Box::into_raw(Box::new(RegNode {
            ring,
            next: AtomicPtr::new(null_mut()),
        }));
        let mut head = self.head.load(Ordering::Relaxed);
        loop {
            unsafe { (&*node).next.store(head, Ordering::Relaxed) };
            // Release publishes the node fields and the fully built ring.
            match self
                .head
                .compare_exchange(head, node, Ordering::Release, Ordering::Relaxed)
            {
                Ok(_) => return,
                Err(observed) => head = observed,
            }
        }
    }

    /// Walks every registered ring (any thread; Acquire loads publish the
    /// nodes and rings).
    pub(crate) fn for_each(&self, mut f: impl FnMut(&'static Ring)) {
        let mut node = self.head.load(Ordering::Acquire);
        while !node.is_null() {
            let n = unsafe { &*node };
            f(unsafe { &*n.ring });
            node = n.next.load(Ordering::Acquire);
        }
    }

    /// One consumer sweep (§3.4): drain `Active` rings; drain `Orphaned`
    /// rings to empty and pool them; skip `Pooled` rings. Returns whether any
    /// ring yielded bytes.
    ///
    /// `sink` may call [`Ring::engine_id`] on the ring it is handed: the
    /// bytes in hand are proof of drain progress, which is that method's
    /// safety contract.
    ///
    /// # Safety
    /// Caller is the process's single consumer thread.
    pub(crate) unsafe fn sweep(&self, sink: &mut impl FnMut(&'static Ring, &[u8])) -> bool {
        let mut progress = false;
        self.for_each(|ring| match ring.state() {
            RingState::Active => {
                progress |= unsafe { ring.drain(&mut |bytes| sink(ring, bytes)) };
            }
            RingState::Orphaned => {
                // The state Acquire (orphan edge) made every pre-death push
                // visible, and the producer is gone — one drain reaches
                // empty.
                progress |= unsafe { ring.drain(&mut |bytes| sink(ring, bytes)) };
                unsafe { ring.mark_pooled() };
            }
            RingState::Pooled => {}
        });
        progress
    }
}

impl Drop for Registry {
    /// Production registries live in statics and never drop. Tests drop
    /// their own after quiescing every producer and the consumer; the
    /// registry then owns the nodes *and* the rings.
    fn drop(&mut self) {
        let mut node = self.head.load(Ordering::Relaxed);
        while !node.is_null() {
            let boxed = unsafe { Box::from_raw(node) };
            drop(unsafe { Box::from_raw(boxed.ring) });
            node = boxed.next.load(Ordering::Relaxed);
        }
    }
}

#[cfg(all(not(baml_loom), not(target_arch = "wasm32")))]
pub(crate) use global::global_ctx;
#[cfg(not(baml_loom))]
pub(crate) use global::global_registry;
#[cfg(not(baml_loom))]
pub use global::ring_for_engine;

#[cfg(not(baml_loom))]
mod global {
    use std::{cell::RefCell, sync::OnceLock};

    use smallvec::SmallVec;

    use super::Registry;
    use crate::prof::{
        config::ProfConfig,
        ring::{Ring, RingCtx, RingHandle},
    };

    static REGISTRY: Registry = Registry::new();

    pub(crate) fn global_registry() -> &'static Registry {
        &REGISTRY
    }

    /// Process-wide ring context, sized and policied from the environment
    /// knobs on first use.
    pub(crate) fn global_ctx() -> &'static RingCtx {
        static CTX: OnceLock<RingCtx> = OnceLock::new();
        CTX.get_or_init(|| {
            let cfg = ProfConfig::global();
            RingCtx::with_policy(cfg.max_overflow_bytes, cfg.exhaustion)
        })
    }

    /// One entry per engine this thread has produced for. The `Drop` is the
    /// D5b orphan trigger, run by the TLS destructor on thread death.
    struct ThreadRings(RefCell<SmallVec<[(u64, &'static Ring); 2]>>);

    impl Drop for ThreadRings {
        fn drop(&mut self) {
            for (_, ring) in self.0.borrow().iter() {
                ring.orphan();
            }
        }
    }

    thread_local! {
        static THREAD_RINGS: ThreadRings = ThreadRings(RefCell::new(SmallVec::new()));
    }

    /// The D5a resume-site lookup: the engine calls this once per exec
    /// resume (never per push) and snapshots the handle into the VM. The TLS
    /// hit is the steady state; a miss claims a pooled ring or allocates and
    /// registers a fresh one.
    ///
    /// Must run on a live thread (not from TLS destructors): the returned
    /// handle's ring is orphaned by *this thread's* TLS cleanup, which is
    /// what guarantees its events eventually reach the consumer.
    pub fn ring_for_engine(engine_id: u64) -> RingHandle {
        THREAD_RINGS.with(|tr| {
            let mut entries = tr.0.borrow_mut();
            if let Some((_, ring)) = entries.iter().find(|(id, _)| *id == engine_id) {
                // SAFETY: this thread claimed the ring when it inserted the
                // entry, and only this thread's death (TLS drop) releases it.
                return unsafe { RingHandle::new(ring) };
            }
            let cfg = ProfConfig::global();
            // Pin the clock anchor (source detection + zero point — cheap,
            // no calibration). Belt-and-braces: now_ticks() also forces it,
            // which is the real every-stamp-postdates-the-anchor invariant.
            crate::prof::clock::init();
            // The consumer drains every registered ring; it must exist
            // before the first event can pile up. No-op when profiling is
            // off (tests drive private registries as their own consumers).
            #[cfg(not(target_arch = "wasm32"))]
            if cfg.is_enabled() {
                crate::prof::consumer::ensure_started();
            }
            let handle = REGISTRY.acquire(global_ctx(), cfg.seg_bytes, cfg.freelist_cap, engine_id);
            entries.push((engine_id, handle.ring()));
            handle
        })
    }
}
