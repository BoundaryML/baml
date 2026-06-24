//! The segmented SPSC ring (plan §3; design decisions D1–D4, D7).
//!
//! One ring per `(engine, os-thread)` pair. Exactly one producer thread (the
//! claimant — see [`crate::prof::registry`] for the claim protocol) and one
//! process-wide consumer thread. Lossless by growth: a full segment links a
//! fresh or recycled segment; nothing ever drops or blocks. Rings are
//! allocated once and never freed (`&'static`), which is what makes the
//! orphan/drain/claim lifecycle race-free without epochs or hazard pointers.
//!
//! # Memory-ordering map (the loom checklist, plan §3.7)
//!
//! | edge            | Release                         | Acquire                       | publishes |
//! |-----------------|---------------------------------|-------------------------------|-----------|
//! | record publish  | `commit_len.store` (producer)   | `commit_len.load` (consumer)  | record bytes ≤ `commit_len` |
//! | segment link    | `next.store` (producer)         | `next.load` (consumer)        | final `commit_len` of the old segment; D2 resets + first bytes of the new one |
//! | segment recycle | free-list push CAS (consumer)   | free-list pop (producer)      | consumer is done reading the segment |
//! | orphan          | `state.store(Orphaned)` (producer thread dtor) | `state.load` (consumer) | every record pushed before thread death |
//! | pool / claim    | `state.store(Pooled)` (consumer) | claim CAS (new producer)     | consumer is done draining; transitively, the dead producer's position fields |
//!
//! # Invariants enforced here (plan §6)
//!
//! - The producer never blocks: no mutex, no condvar, no park, no unbounded
//!   spin anywhere reachable from [`Ring::push`]. It runs holding an
//!   `ActiveHeapPermit`; a blocked producer is an engine-wide GC stall.
//! - The consumer never touches the GC heap or permits.
//! - Hitting the live-memory cap is a hard process error with a clear
//!   message (D6), never a silent drop.
#![allow(unsafe_code)]
// On wasm32 there is no background consumer thread. The consumer-side helpers
// are compiled for the cooperative drain path, but not all native-only entry
// points call them directly.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use std::{marker::PhantomData, ptr::null_mut};

use crossbeam_utils::CachePadded;

use crate::prof::{
    sync::{AtomicPtr, AtomicU8, AtomicU32, AtomicUsize, Buf, Ordering, UnsafeCell},
    wake::Wake,
};

/// Ring lifecycle states (design D5b).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum RingState {
    /// Claimed by a live producer thread.
    Active = 0,
    /// The producer thread died; the consumer must drain to empty, then pool.
    Orphaned = 1,
    /// Drained empty and claimable by a new producer (CAS Pooled→Active).
    Pooled = 2,
}

impl RingState {
    fn from_u8(v: u8) -> RingState {
        match v {
            0 => RingState::Active,
            1 => RingState::Orphaned,
            2 => RingState::Pooled,
            _ => unreachable!("invalid RingState discriminant {v}"),
        }
    }
}

/// Process-wide context shared by all rings: the live-memory budget and the
/// consumer wake state.
pub(crate) struct RingCtx {
    budget: MemBudget,
    wake: Wake,
}

impl RingCtx {
    #[cfg(not(baml_loom))]
    pub(crate) const fn new(max_overflow_bytes: usize) -> Self {
        Self {
            budget: MemBudget::new(max_overflow_bytes),
            wake: Wake::new(),
        }
    }

    #[cfg(baml_loom)]
    pub(crate) fn new(max_overflow_bytes: usize) -> Self {
        Self {
            budget: MemBudget::new(max_overflow_bytes),
            wake: Wake::new(),
        }
    }

    pub(crate) fn wake(&self) -> &Wake {
        &self.wake
    }

    /// Currently allocated ring-segment bytes (approximate bookkeeping unit:
    /// buffer + segment header). Test/telemetry surface — production code
    /// only writes the budget.
    #[cfg_attr(not(test), allow(dead_code, reason = "test/telemetry accessor"))]
    pub(crate) fn live_bytes(&self) -> usize {
        self.budget.live()
    }
}

/// Live-memory accounting against `BAML_RING_MAX_OVERFLOW_BYTES` (D6).
pub(crate) struct MemBudget {
    live: AtomicUsize,
    cap: usize,
}

impl MemBudget {
    #[cfg(not(baml_loom))]
    const fn new(cap: usize) -> Self {
        Self {
            live: AtomicUsize::new(0),
            cap,
        }
    }

    #[cfg(baml_loom)]
    fn new(cap: usize) -> Self {
        Self {
            live: AtomicUsize::new(0),
            cap,
        }
    }

    fn charge(&self, bytes: usize) {
        let live = self.live.fetch_add(bytes, Ordering::Relaxed) + bytes;
        if live > self.cap {
            overflow_abort(live, self.cap);
        }
    }

    fn credit(&self, bytes: usize) {
        self.live.fetch_sub(bytes, Ordering::Relaxed);
    }

    #[cfg_attr(not(test), allow(dead_code, reason = "test/telemetry accessor"))]
    fn live(&self) -> usize {
        self.live.load(Ordering::Relaxed)
    }
}

/// D6: exceeding the cap means the consumer cannot keep up with the
/// sustained event rate — a hard process error, stated plainly, never a
/// silent drop (and never an opaque OOM kill later).
#[cold]
#[expect(clippy::print_stderr, reason = "process-fatal diagnostic")]
fn overflow_abort(live: usize, cap: usize) -> ! {
    eprintln!(
        "FATAL: BAML profiling ring memory ({live} bytes) exceeded \
         BAML_RING_MAX_OVERFLOW_BYTES ({cap} bytes).\n\
         The profile consumer cannot keep up with the sustained event rate. \
         Raise BAML_RING_MAX_OVERFLOW_BYTES, lower the event rate, or disable \
         profiling (BAML_PROFILE=0). Aborting instead of growing without \
         bound or silently dropping events."
    );
    std::process::abort();
}

struct SegSync {
    /// Bytes of the segment published so far. Producer `Release`-stores after
    /// each record's bytes are written; monotonic for the lifetime of one
    /// fill (reset only by the producer when re-linking a recycled segment).
    commit_len: AtomicU32,
    /// Next segment in the chain (`Release` by producer on link), or — while
    /// the segment sits in the free list — the next free-list node. A
    /// segment is never in both places, and in-list nodes are immutable
    /// until popped, so the dual use cannot be observed concurrently.
    next: AtomicPtr<Segment>,
}

struct Segment {
    /// D3: `{commit_len, next}` get their own cache line so producer commits
    /// and consumer polls don't false-share with the buffer pointer reads.
    sync: CachePadded<SegSync>,
    buf: Buf,
}

fn segment_footprint(seg_bytes: usize) -> usize {
    seg_bytes + size_of::<Segment>()
}

fn alloc_segment(seg_bytes: usize, ctx: &RingCtx) -> *mut Segment {
    ctx.budget.charge(segment_footprint(seg_bytes));
    Box::into_raw(Box::new(Segment {
        sync: CachePadded::new(SegSync {
            commit_len: AtomicU32::new(0),
            next: AtomicPtr::new(null_mut()),
        }),
        buf: Buf::new(seg_bytes),
    }))
}

/// # Safety
/// `seg` came from [`alloc_segment`], is reachable from neither the live
/// chain nor the free list, and no thread will touch it again.
unsafe fn free_segment(seg: *mut Segment, ctx: &RingCtx) {
    let boxed = unsafe { Box::from_raw(seg) };
    ctx.budget.credit(segment_footprint(boxed.buf.capacity()));
}

/// Producer-only position fields (D3 group 1).
struct RingProducer {
    head: *mut Segment,
    head_pos: usize,
}

/// Consumer-only position fields (D3 group 2).
struct RingConsumer {
    tail: *mut Segment,
    tail_pos: usize,
}

/// Shared fields, touched roughly once per segment (D3 group 3).
struct RingShared {
    state: AtomicU8,
    /// Treiber free list of recycled segments. Single pusher (the consumer)
    /// and single popper (the producer) — see the no-ABA argument on
    /// [`Ring::free_pop`].
    free_head: AtomicPtr<Segment>,
    /// Approximate free-list length (Relaxed; D7). Transient off-by-one —
    /// including the brief wrap-below-zero when a pop's decrement lands
    /// between a push's CAS and its increment — only skews the recycle/free
    /// choice, which is explicitly tolerant of that.
    free_len: AtomicUsize,
    /// Which engine the claimant produces for. Written by the claimant
    /// *before* its first push, read by the consumer only *after* observing
    /// a new commit — so the first push's Release/Acquire publishes it.
    engine_id: UnsafeCell<u64>,
}

/// The segmented SPSC ring. See the module docs.
pub struct Ring {
    p: CachePadded<UnsafeCell<RingProducer>>,
    c: CachePadded<UnsafeCell<RingConsumer>>,
    s: CachePadded<RingShared>,
    /// Immutable after construction.
    seg_bytes: usize,
    freelist_cap: usize,
    ctx: &'static RingCtx,
}

// SAFETY: all cross-thread access is mediated by the role contracts —
// `p` is touched only by the unique producer (claim protocol, registry),
// `c` only by the single consumer thread, `s.engine_id` per the
// publish-by-first-commit rule — with the Release/Acquire edges in the
// module-docs table providing the happens-before. The loom suite
// model-checks exactly these claims.
unsafe impl Send for Ring {}
unsafe impl Sync for Ring {}

impl Ring {
    /// Allocates a ring with one open segment, `Active` and owned by the
    /// calling thread (the creator is the claimant). The returned pointer is
    /// conceptually `&'static`: production code never frees rings
    /// (invariant 7); tests reclaim through this pointer after quiescing.
    pub(crate) fn alloc(
        ctx: &'static RingCtx,
        seg_bytes: usize,
        freelist_cap: usize,
        engine_id: u64,
    ) -> *mut Ring {
        let seg = alloc_segment(seg_bytes, ctx);
        Box::into_raw(Box::new(Ring {
            p: CachePadded::new(UnsafeCell::new(RingProducer {
                head: seg,
                head_pos: 0,
            })),
            c: CachePadded::new(UnsafeCell::new(RingConsumer {
                tail: seg,
                tail_pos: 0,
            })),
            s: CachePadded::new(RingShared {
                state: AtomicU8::new(RingState::Active as u8),
                free_head: AtomicPtr::new(null_mut()),
                free_len: AtomicUsize::new(0),
                engine_id: UnsafeCell::new(engine_id),
            }),
            seg_bytes,
            freelist_cap,
            ctx,
        }))
    }

    /// The producer hot path (§3.2): bounds check + `memcpy` + one `Release`
    /// store per record. The slow path (segment link + possible consumer
    /// wake) runs about once per `seg_bytes / record_len` pushes. No TLS, no
    /// global loads, no CAS in steady state, no allocation while the free
    /// list can supply a segment — and never, ever a block.
    ///
    /// # Safety
    /// Caller is the ring's unique producer thread (it claimed the ring and
    /// the ring is `Active`), and `rec` is a whole encoded record with
    /// `0 < rec.len() <= seg_bytes`.
    pub unsafe fn push(&self, rec: &[u8]) {
        debug_assert!(!rec.is_empty());
        // SAFETY: same producer contract as `push_with`; the closure copies
        // exactly `rec.len()` bytes into the (uninitialized) slot.
        unsafe {
            self.push_with(rec.len(), |slot| slot.copy_from_slice(rec));
        }
    }

    /// Reserve `len` bytes in the ring and let the producer serialize a record
    /// *directly into them* via `write`, avoiding the encode-to-stack-buffer +
    /// copy-into-ring double write that [`Self::push`] pays. `write` must
    /// initialize all `len` bytes (they are committed immediately after it
    /// returns).
    ///
    /// # Safety
    /// Same contract as [`Self::push`]: this OS thread is the ring's live producer
    /// (D5a refresh), and exec does not cross an `.await` (plan §6 inv. 4).
    #[inline]
    pub unsafe fn push_with(&self, len: usize, write: impl FnOnce(&mut [u8])) {
        debug_assert!(len > 0);
        self.p.with_mut(|p| {
            let p = unsafe { &mut *p };
            if p.head_pos + len > self.seg_bytes {
                // An oversized record necessarily lands here (head_pos ≥ 0),
                // so this release-mode check costs one compare per segment
                // fill — never per push — and is what keeps the write below
                // in bounds for arbitrary input.
                assert!(
                    len <= self.seg_bytes,
                    "profiling record ({} bytes) exceeds the ring segment size ({} bytes)",
                    len,
                    self.seg_bytes
                );
                // Slow path: link a recycled or fresh segment.
                let seg = match unsafe { self.free_pop() } {
                    Some(seg) => seg,
                    None => alloc_segment(self.seg_bytes, self.ctx),
                };
                unsafe {
                    // D2: the producer owns the reset, for recycled and fresh
                    // segments alike. Relaxed is enough: the link store below
                    // publishes these to the consumer, and the free-list pop
                    // (Acquire) ordered us after the consumer's last read of
                    // a recycled segment.
                    let new_seg = &*seg;
                    new_seg.sync.commit_len.store(0, Ordering::Relaxed);
                    new_seg.sync.next.store(null_mut(), Ordering::Relaxed);
                    // The link: publishes the resets above and — being
                    // sequenced after every commit to the old head — that
                    // segment's *final* commit_len (D1).
                    (&*p.head).sync.next.store(seg, Ordering::Release);
                }
                p.head = seg;
                p.head_pos = 0;
                // D4: wake only on segment fill, only if the consumer is
                // parked. Wake, not wait — this cannot block.
                self.ctx.wake.wake_if_parked();
            }
            unsafe {
                let head = &*p.head;
                head.buf.with_slice_mut(p.head_pos, len, write);
                p.head_pos += len;
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "head_pos <= seg_bytes <= 16 MiB, far below u32::MAX"
                )]
                head.sync
                    .commit_len
                    .store(p.head_pos as u32, Ordering::Release);
            }
        });
    }

    /// Consumer-side drain (§3.3, with D1 baked in). Hands `sink` each newly
    /// published byte range — always a whole number of records, because the
    /// producer commits whole records. Returns whether any bytes were
    /// consumed.
    ///
    /// # Safety
    /// Caller is the process's single consumer thread.
    pub(crate) unsafe fn drain(&self, sink: &mut impl FnMut(&[u8])) -> bool {
        self.c.with_mut(|c| {
            let c = unsafe { &mut *c };
            let mut progress = false;
            loop {
                let seg = c.tail;
                let seg_ref = unsafe { &*seg };
                let committed = seg_ref.sync.commit_len.load(Ordering::Acquire);
                progress |=
                    unsafe { consume_range(seg_ref, &mut c.tail_pos, committed as usize, sink) };
                let next = seg_ref.sync.next.load(Ordering::Acquire);
                if next.is_null() {
                    return progress; // still the open segment; caught up for now
                }
                // D1: a non-null `next` means the producer is done with `seg`
                // forever, and the Acquire above (pairing with the link
                // Release) made its *final* commit_len visible. Re-load and
                // drain the remainder before retiring.
                let committed = seg_ref.sync.commit_len.load(Ordering::Acquire);
                progress |=
                    unsafe { consume_range(seg_ref, &mut c.tail_pos, committed as usize, sink) };
                unsafe { self.retire(seg) };
                c.tail = next;
                c.tail_pos = 0;
            }
        })
    }

    /// Producer-side free-list pop.
    ///
    /// No ABA despite the bare Treiber loop: this thread is the *only*
    /// popper (SPSC), and a node's `next` only changes when the single
    /// pusher writes it *before* pushing — a node already on the list is
    /// immutable until we pop it. The classic ABA interleaving requires a
    /// concurrent popper to remove and re-push the observed head; that
    /// thread does not exist.
    ///
    /// # Safety
    /// Caller is the ring's unique producer thread.
    unsafe fn free_pop(&self) -> Option<*mut Segment> {
        let mut head = self.s.free_head.load(Ordering::Acquire);
        loop {
            if head.is_null() {
                return None;
            }
            // Visible: the Release push that made `head` reachable stored
            // its `next` first.
            let next = unsafe { (&*head).sync.next.load(Ordering::Relaxed) };
            // Acquire on success: pairs with the consumer's Release push —
            // the consumer's last read of this segment happens-before our
            // reuse of it (D2's safety chain).
            match self.s.free_head.compare_exchange(
                head,
                next,
                Ordering::Acquire,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    self.s.free_len.fetch_sub(1, Ordering::Relaxed);
                    return Some(head);
                }
                Err(observed) => head = observed,
            }
        }
    }

    /// Consumer-side segment retirement (D7): push to the free list, or free
    /// outright when the list is at cap.
    ///
    /// # Safety
    /// Caller is the consumer thread, has fully drained `seg`, and has
    /// already advanced `tail` past it (the producer linked past it, so
    /// neither side can reach it again).
    unsafe fn retire(&self, seg: *mut Segment) {
        if self.s.free_len.load(Ordering::Relaxed) >= self.freelist_cap {
            unsafe { free_segment(seg, self.ctx) };
            return;
        }
        let mut head = self.s.free_head.load(Ordering::Relaxed);
        loop {
            unsafe { (&*seg).sync.next.store(head, Ordering::Relaxed) };
            // Release: publishes "the consumer is done reading `seg`" (plus
            // the next-link above) to the producer's pop.
            match self
                .s
                .free_head
                .compare_exchange(head, seg, Ordering::Release, Ordering::Relaxed)
            {
                Ok(_) => break,
                Err(observed) => head = observed,
            }
        }
        self.s.free_len.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn state(&self) -> RingState {
        RingState::from_u8(self.s.state.load(Ordering::Acquire))
    }

    /// Producer-thread-death hook (D5b), called from the TLS-map destructor.
    /// The Release pairs with the consumer's `state` Acquire: every record
    /// pushed before death is visible to the post-orphan drain.
    pub(crate) fn orphan(&self) {
        debug_assert_eq!(
            self.state(),
            RingState::Active,
            "only the live claimant's thread death orphans a ring"
        );
        self.s
            .state
            .store(RingState::Orphaned as u8, Ordering::Release);
    }

    /// Consumer: mark an orphaned ring claimable after draining it to empty.
    /// The Release pairs with a claimant's CAS — transitively ordering the
    /// dead producer's position fields before the new producer's first push.
    ///
    /// # Safety
    /// Caller is the consumer thread and just drained this ring to empty
    /// after observing it `Orphaned`.
    pub(crate) unsafe fn mark_pooled(&self) {
        debug_assert_eq!(
            self.state(),
            RingState::Orphaned,
            "only a drained orphan can be pooled"
        );
        self.s
            .state
            .store(RingState::Pooled as u8, Ordering::Release);
    }

    /// New-producer claim: CAS `Pooled → Active`. On success the caller is
    /// the unique producer and may push from the calling thread only.
    ///
    /// Acquire on success: pairs with [`Ring::mark_pooled`]'s Release, so the
    /// consumer's final drain — and, through the orphan edge before it, the
    /// dead producer's last `head`/`head_pos` writes — happen-before the new
    /// producer touches them.
    pub(crate) fn try_claim(&self, engine_id: u64) -> bool {
        if self
            .s
            .state
            .compare_exchange(
                RingState::Pooled as u8,
                RingState::Active as u8,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_ok()
        {
            // Sound despite no fence here: the consumer reads engine_id only
            // after observing a commit_len advance, and our first push's
            // Release is sequenced after this write.
            self.s.engine_id.with_mut(|p| unsafe { *p = engine_id });
            true
        } else {
            false
        }
    }

    /// Which engine the bytes most recently drained from this ring belong to.
    ///
    /// # Safety
    /// Consumer thread only, and only after a [`Ring::drain`] on this ring
    /// reported progress in the current sweep (that drain's Acquire is what
    /// publishes a new claimant's write).
    pub(crate) unsafe fn engine_id(&self) -> u64 {
        self.s.engine_id.with(|p| unsafe { *p })
    }

    #[cfg(test)]
    pub(crate) fn free_len(&self) -> usize {
        self.s.free_len.load(Ordering::Relaxed)
    }
}

impl Drop for Ring {
    /// Production rings are never dropped (invariant 7: `&'static`, reuse
    /// don't reclaim). This exists for tests, which reclaim rings after all
    /// producer/consumer activity has quiesced; then the live chain
    /// (`tail → … → head`) and the free list are disjoint sets covering
    /// every segment this ring still owns.
    fn drop(&mut self) {
        unsafe {
            let mut seg = self.c.with_mut(|c| (*c).tail);
            while !seg.is_null() {
                let next = (&*seg).sync.next.load(Ordering::Relaxed);
                free_segment(seg, self.ctx);
                seg = next;
            }
            let mut free = self.s.free_head.load(Ordering::Relaxed);
            while !free.is_null() {
                let next = (&*free).sync.next.load(Ordering::Relaxed);
                free_segment(free, self.ctx);
                free = next;
            }
        }
    }
}

/// The producer-side write handle: living proof that the holding thread
/// claimed the ring. Created only by the claim/acquire paths on the claiming
/// thread, and `!Send + !Sync` so it cannot leave it — which is what makes
/// [`RingHandle::push`] safe to expose.
#[derive(Clone, Copy)]
pub struct RingHandle {
    ring: &'static Ring,
    _not_send_sync: PhantomData<*mut ()>,
}

impl RingHandle {
    /// # Safety
    /// `ring` is `Active` and was claimed by (or created on) the calling
    /// thread, which makes that thread the unique producer.
    pub(crate) unsafe fn new(ring: &'static Ring) -> Self {
        Self {
            ring,
            _not_send_sync: PhantomData,
        }
    }

    /// Push one whole encoded record. Never blocks. Oversized records
    /// (> segment size) panic on the slow path rather than corrupting the
    /// ring.
    ///
    /// # Safety
    /// The calling thread must still be the ring's live claimant: not after
    /// its `ThreadRings` TLS destructor ran (which orphans the ring and lets
    /// a new thread claim it — a later push would race the new producer).
    /// `!Send` pins the handle to the claiming thread, but TLS destructor
    /// ordering is unspecified, so "before TLS teardown" cannot be enforced
    /// by the type. The engine upholds this via the per-resume refresh
    /// (D5a): handles are re-fetched from live TLS each exec resume and
    /// never used from destructors.
    #[inline]
    pub unsafe fn push(self, rec: &[u8]) {
        // SAFETY: !Send pins us to the claiming thread; the caller vouches
        // the claim is still live (contract above).
        unsafe { self.ring.push(rec) }
    }

    /// The underlying ring, for D5a snapshots: the engine stores this
    /// `&'static Ring` in the VM and refreshes it once per exec resume.
    #[must_use]
    pub fn ring(self) -> &'static Ring {
        self.ring
    }
}

/// Parses nothing: hands `sink` the raw committed range `[tail_pos,
/// committed)` and advances `tail_pos`. Bytes below `committed` were
/// published by the Acquire that read it and are immutable until recycle.
unsafe fn consume_range(
    seg: &Segment,
    tail_pos: &mut usize,
    committed: usize,
    sink: &mut impl FnMut(&[u8]),
) -> bool {
    debug_assert!(committed >= *tail_pos, "commit_len went backwards");
    if committed <= *tail_pos {
        return false;
    }
    let len = committed - *tail_pos;
    unsafe { seg.buf.with_slice(*tail_pos, len, |bytes| sink(bytes)) };
    *tail_pos = committed;
    true
}
