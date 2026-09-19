//! Copied append-only ring registry, with Btel round-robin traversal.
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

use crate::{
    ring::{OSThreadMarkerRing, OSThreadMarkerRingHandle, RingCtx, RingState},
    sync::{AtomicPtr, Ordering},
};

struct RegNode {
    /// Conceptually `&'static OSThreadMarkerRing`; kept raw so tests can reclaim with full
    /// provenance. Btel reclaims it after all owning endpoints are gone.
    ring: *mut OSThreadMarkerRing,
    next: AtomicPtr<RegNode>,
}

/// `AtomicPtr` retains pointer provenance and permits moving the sole consumer
/// endpoint to its worker. Only that endpoint reads/writes this cursor.
#[derive(Default)]
pub(crate) struct Cursor {
    next: AtomicPtr<RegNode>,
}

pub(crate) struct Visit {
    pub progress: bool,
    pub wrapped: bool,
}

/// Append-only ring registry. One per Btel transport instance.
pub(crate) struct Registry {
    head: AtomicPtr<RegNode>,
    tail: AtomicPtr<RegNode>,
}

impl Registry {
    /// Visit one source, retaining our next position across bounded steps.
    /// Caller must be the registry's unique consumer.
    pub(crate) unsafe fn visit_next(
        &self,
        cursor: &mut Cursor,
        sink: &mut impl FnMut(&'static OSThreadMarkerRing, &[u8]),
    ) -> Option<Visit> {
        let mut node = cursor.next.load(Ordering::Relaxed);
        if node.is_null() {
            node = self.head.load(Ordering::Acquire);
        }
        if node.is_null() {
            return None;
        }
        let n = unsafe { &*node };
        let ring = unsafe { &*n.ring };
        let progress = match ring.state() {
            RingState::Active => unsafe { ring.drain(&mut |bytes| sink(ring, bytes)) }.progress,
            RingState::Orphaned => {
                let outcome = unsafe { ring.drain(&mut |bytes| sink(ring, bytes)) };
                if outcome.caught_up {
                    unsafe { ring.mark_pooled() };
                }
                outcome.progress
            }
            RingState::Pooled => false,
        };
        let next = n.next.load(Ordering::Acquire);
        cursor.next.store(next, Ordering::Relaxed);
        Some(Visit {
            progress,
            wrapped: next.is_null(),
        })
    }
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
    ) -> Option<OSThreadMarkerRingHandle> {
        let mut node = self.head.load(Ordering::Acquire);
        while !node.is_null() {
            let n = unsafe { &*node };
            let ring = unsafe { &*n.ring };
            if ring.try_claim(engine_id) {
                // SAFETY: the CAS made this thread the unique producer.
                return Some(unsafe { OSThreadMarkerRingHandle::new(ring) });
            }
            node = n.next.load(Ordering::Acquire);
        }
        // Keep the raw pointer (not a ref-derived copy) in the node so the
        // Registry::drop deallocates with original provenance.
        let ring_ptr = OSThreadMarkerRing::alloc(ctx, seg_bytes, freelist_cap, engine_id)?;
        self.push(ring_ptr);
        // SAFETY: a freshly allocated ring is Active and owned by its
        // creating thread.
        Some(unsafe { OSThreadMarkerRingHandle::new(&*ring_ptr) })
    }

    fn push(&self, ring: *mut OSThreadMarkerRing) {
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
            // allocated for the registry lifetime, and this appender uniquely
            // owns its one transition from a null `next` pointer.
            unsafe { (&*previous).next.store(node, Ordering::Release) };
        }
    }

    /// Walks every registered ring (any thread; Acquire loads publish the
    /// nodes and rings).
    pub(crate) fn for_each(&self, mut f: impl FnMut(&'static OSThreadMarkerRing)) {
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
    /// `sink` may call [`OSThreadMarkerRing::engine_id`] on the ring it is handed: the
    /// bytes in hand are proof of drain progress, which is that method's
    /// safety contract.
    ///
    /// # Safety
    /// Caller is the process's single consumer thread.
    #[cfg(test)]
    pub(crate) unsafe fn sweep(
        &self,
        sink: &mut impl FnMut(&'static OSThreadMarkerRing, &[u8]),
    ) -> bool {
        unsafe { self.sweep_outcome(sink) }.progress
    }

    /// Return both byte progress and whether every visited source caught up.
    /// Caller is the registry's unique consumer.
    pub(crate) unsafe fn sweep_outcome(
        &self,
        sink: &mut impl FnMut(&'static OSThreadMarkerRing, &[u8]),
    ) -> crate::ring::DrainOutcome {
        let mut progress = false;
        let mut caught_up = true;
        self.for_each(|ring| match ring.state() {
            RingState::Active => {
                let outcome = unsafe { ring.drain(&mut |bytes| sink(ring, bytes)) };
                progress |= outcome.progress;
                caught_up &= outcome.caught_up;
            }
            RingState::Orphaned => {
                // The state Acquire (orphan edge) made every pre-death push
                // visible, and the producer is gone — a caught-up drain has
                // reached empty. A drain stopped by its segment bound leaves
                // the ring Orphaned for the next sweep.
                let outcome = unsafe { ring.drain(&mut |bytes| sink(ring, bytes)) };
                progress |= outcome.progress;
                caught_up &= outcome.caught_up;
                if outcome.caught_up {
                    unsafe { ring.mark_pooled() };
                }
            }
            RingState::Pooled => {}
        });
        crate::ring::DrainOutcome {
            progress,
            caught_up,
        }
    }
}

impl Drop for Registry {
    /// The runtime retains this registry through every endpoint. Once all
    /// endpoints are gone, it owns and reclaims the nodes and rings. Copied
    /// tests must likewise quiesce all users before dropping their registry.
    fn drop(&mut self) {
        let mut node = self.head.load(Ordering::Relaxed);
        while !node.is_null() {
            let boxed = unsafe { Box::from_raw(node) };
            drop(unsafe { Box::from_raw(boxed.ring) });
            node = boxed.next.load(Ordering::Relaxed);
        }
    }
}
