//! The ring registry and per-thread ring map (plan §3.4, design D5b).
//!
//! The registry is an append-only FIFO list: rings are linked once and never
//! removed (no pop ⇒ no ABA; no reclamation ⇒ no epochs). FIFO traversal
//! preserves causal registration order, so a parent ring that creates worker
//! rings is swept before those descendants. Thread death *orphans* a ring;
//! the consumer drains it to empty
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
    tail: AtomicPtr<RegNode>,
}

impl Registry {
    #[cfg(not(baml_loom))]
    pub(crate) const fn new() -> Self {
        Self {
            head: AtomicPtr::new(null_mut()),
            tail: AtomicPtr::new(null_mut()),
        }
    }

    #[cfg(baml_loom)]
    pub(crate) fn new() -> Self {
        Self {
            head: AtomicPtr::new(null_mut()),
            tail: AtomicPtr::new(null_mut()),
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
    ) -> Option<RingHandle> {
        let mut node = self.head.load(Ordering::Acquire);
        while !node.is_null() {
            let n = unsafe { &*node };
            let ring = unsafe { &*n.ring };
            if ring.try_claim(engine_id) {
                // SAFETY: the CAS made this thread the unique producer.
                return Some(unsafe { RingHandle::new(ring) });
            }
            node = n.next.load(Ordering::Acquire);
        }
        // Keep the raw pointer (not a ref-derived copy) in the node so the
        // test-only Registry::drop deallocates with original provenance.
        let ring_ptr = Ring::alloc(ctx, seg_bytes, freelist_cap, engine_id)?;
        self.push(ring_ptr);
        // SAFETY: a freshly allocated ring is Active and owned by its
        // creating thread.
        Some(unsafe { RingHandle::new(&*ring_ptr) })
    }

    fn push(&self, ring: *mut Ring) {
        let node = Box::into_raw(Box::new(RegNode {
            ring,
            next: AtomicPtr::new(null_mut()),
        }));
        // The tail exchange is the append linearization point. A concurrent
        // consumer may miss a node whose predecessor link is not published
        // yet, but the next sweep observes it; nodes are never removed.
        let previous = self.tail.swap(node, Ordering::AcqRel);
        if previous.is_null() {
            self.head.store(node, Ordering::Release);
        } else {
            // SAFETY: `previous` is an append-only registry node that remains
            // allocated for the process lifetime, and this appender uniquely
            // owns its one transition from a null `next` pointer.
            unsafe { (&*previous).next.store(node, Ordering::Release) };
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
pub(crate) use global::global_registry;
#[cfg(not(baml_loom))]
pub use global::ring_for_engine;
#[cfg(all(not(baml_loom), not(target_arch = "wasm32")))]
pub(crate) use global::{configure_global_transport, global_ctx};

#[cfg(not(baml_loom))]
mod global {
    use std::{cell::RefCell, sync::OnceLock};

    use smallvec::SmallVec;

    use super::Registry;
    use crate::prof::{
        backend::{MeasuredLayouts, ProfilerMemoryGovernor, ProfilerSizingPolicy},
        ring::{Ring, RingCtx, RingHandle},
    };

    static REGISTRY: Registry = Registry::new();

    #[derive(Clone)]
    struct TransportConfig {
        memory: ProfilerMemoryGovernor,
        segment_bytes: usize,
        freelist_segments: usize,
    }

    static TRANSPORT_CONFIG: OnceLock<TransportConfig> = OnceLock::new();

    pub(crate) fn global_registry() -> &'static Registry {
        &REGISTRY
    }

    pub(crate) fn configure_global_transport(
        memory: ProfilerMemoryGovernor,
        segment_bytes: u64,
        freelist_segments: u32,
    ) {
        let _ = TRANSPORT_CONFIG.set(TransportConfig {
            memory,
            segment_bytes: usize::try_from(segment_bytes).unwrap_or(usize::MAX),
            freelist_segments: usize::try_from(freelist_segments).unwrap_or(usize::MAX),
        });
    }

    fn transport_config() -> TransportConfig {
        TRANSPORT_CONFIG
            .get_or_init(|| {
                let sizing = ProfilerSizingPolicy::derive(
                    crate::prof::backend::ProfilerConfig::default().process_memory_bytes,
                    MeasuredLayouts::V1,
                )
                .expect("default profiler sizing is valid");
                TransportConfig {
                    memory: ProfilerMemoryGovernor::new(sizing, MeasuredLayouts::V1),
                    segment_bytes: usize::try_from(sizing.transport_segment_bytes)
                        .unwrap_or(usize::MAX),
                    freelist_segments: usize::try_from(sizing.transport_freelist_segments)
                        .unwrap_or(usize::MAX),
                }
            })
            .clone()
    }

    /// Process-wide ring context, sized from the first registered session.
    pub(crate) fn global_ctx() -> &'static RingCtx {
        static CTX: OnceLock<RingCtx> = OnceLock::new();
        CTX.get_or_init(|| {
            let config = transport_config();
            RingCtx::with_governor(config.memory)
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
    pub fn ring_for_engine(engine_id: u64) -> Option<RingHandle> {
        THREAD_RINGS.with(|tr| {
            let mut entries = tr.0.borrow_mut();
            if let Some((_, ring)) = entries.iter().find(|(id, _)| *id == engine_id) {
                // SAFETY: this thread claimed the ring when it inserted the
                // entry, and only this thread's death (TLS drop) releases it.
                return Some(unsafe { RingHandle::new(ring) });
            }
            let config = transport_config();
            // Pin the clock anchor (source detection + zero point — cheap,
            // no calibration). Belt-and-braces: now_ticks() also forces it,
            // which is the real every-stamp-postdates-the-anchor invariant.
            crate::prof::clock::init();
            // The consumer drains every registered ring; it must exist
            // before the first event can pile up.
            #[cfg(not(target_arch = "wasm32"))]
            crate::prof::consumer::ensure_started();
            let handle = REGISTRY.acquire(
                global_ctx(),
                config.segment_bytes,
                config.freelist_segments,
                engine_id,
            )?;
            entries.push((engine_id, handle.ring()));
            Some(handle)
        })
    }
}
