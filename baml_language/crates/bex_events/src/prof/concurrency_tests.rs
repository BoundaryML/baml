//! Concurrency tests for the prof ring.
//!
//! The five [`scenarios`] are the loom checklist from the plan (§3.7):
//! D1 hand-off, D2 recycle, free-list SPSC integrity, the
//! orphan→drain→pool→claim lifecycle, and concurrent registry append+claim.
//! Under `RUSTFLAGS="--cfg baml_loom"` they run inside the loom model checker
//! (every interleaving up to the preemption bound); under plain `cargo test`
//! — and `cargo miri test`, which checks the raw-pointer/UnsafeCell
//! discipline — the same scenario code runs on real threads, repeatedly.
//!
//! The [`stress`] module adds the real-thread flood tests: record framing,
//! forced growth/recycling, the wake/park protocol, orphan churn, and the
//! global TLS path.
#![allow(unsafe_code)]

use crate::prof::{
    ring::{Ring, RingCtx, RingState, segment_footprint},
    sync::thread,
};

/// Big enough that no ordinary scenario hits transport admission pressure.
const BIG_CAP: usize = 1 << 40;

fn leak_ctx(cap: usize) -> &'static RingCtx {
    Box::leak(Box::new(RingCtx::new(cap)))
}

/// 8-byte sequence-number records. The ring is format-agnostic (it moves
/// whole records of any layout); `record.rs` framing is exercised by the
/// stress tests.
fn rec(seq: u64) -> [u8; 8] {
    seq.to_le_bytes()
}

fn collect_seqs(bytes: &[u8], out: &mut Vec<u64>) {
    assert_eq!(bytes.len() % 8, 0, "drained range split a record");
    for chunk in bytes.chunks_exact(8) {
        out.push(u64::from_le_bytes(chunk.try_into().unwrap()));
    }
}

/// Owns a ring allocation so scenarios reclaim it (with original pointer
/// provenance) after all threads quiesce. Production never frees rings; the
/// reclaim exists so loom iterations and miri runs stay leak-tight.
struct TestRing {
    ring: *mut Ring,
}

impl TestRing {
    fn new(ctx: &'static RingCtx, seg_bytes: usize, freelist_cap: usize, engine_id: u64) -> Self {
        Self {
            ring: Ring::alloc(ctx, seg_bytes, freelist_cap, engine_id).expect("test ring capacity"),
        }
    }

    fn get(&self) -> &'static Ring {
        unsafe { &*self.ring }
    }
}

impl Drop for TestRing {
    fn drop(&mut self) {
        drop(unsafe { Box::from_raw(self.ring) });
    }
}

mod scenarios {
    use super::*;
    use crate::prof::registry::Registry;

    /// (1) D1 hand-off: the producer links new segments mid-stream while the
    /// consumer drains concurrently; after the producer finishes, one final
    /// drain must account for every record, in order, exactly once.
    pub(super) fn handoff_no_loss() {
        let ctx = leak_ctx(BIG_CAP);
        let tr = TestRing::new(ctx, 16, 8, 1); // two 8-byte records per segment
        let ring = tr.get();
        let producer = thread::spawn(move || {
            for seq in 0..5 {
                // SAFETY: this thread is the creator-claimant; no other
                // thread pushes.
                unsafe { ring.push(&rec(seq)) };
            }
        });
        let mut seen = Vec::new();
        for _ in 0..2 {
            // SAFETY: this thread is the single consumer.
            unsafe { ring.drain(&mut |b| collect_seqs(b, &mut seen)) };
        }
        producer.join().unwrap();
        unsafe { ring.drain(&mut |b| collect_seqs(b, &mut seen)) };
        assert_eq!(seen, (0..5).collect::<Vec<_>>());
    }

    /// (2) D2 recycle under concurrency: one-record segments force a link on
    /// every push, and concurrent drains retire/recycle segments back to the
    /// producer. No interleaving may surface stale `commit_len`/`next` state
    /// (which would show up as lost, duplicated, or reordered records).
    pub(super) fn recycle_no_stale() {
        let ctx = leak_ctx(BIG_CAP);
        let tr = TestRing::new(ctx, 8, 8, 1); // one record per segment
        let ring = tr.get();
        let producer = thread::spawn(move || {
            for seq in 0..4 {
                // SAFETY: unique producer.
                unsafe { ring.push(&rec(seq)) };
            }
        });
        let mut seen = Vec::new();
        for _ in 0..2 {
            // SAFETY: single consumer.
            unsafe { ring.drain(&mut |b| collect_seqs(b, &mut seen)) };
        }
        producer.join().unwrap();
        unsafe { ring.drain(&mut |b| collect_seqs(b, &mut seen)) };
        assert_eq!(seen, (0..4).collect::<Vec<_>>());
    }

    /// (2b) D2 deterministic: recycling actually reuses free-list segments —
    /// growth after a drain must not allocate (live bytes stay flat) and the
    /// reused segments must carry no stale data.
    pub(super) fn recycle_reuses_memory() {
        let ctx = leak_ctx(BIG_CAP);
        let tr = TestRing::new(ctx, 8, 8, 1);
        let ring = tr.get();
        // Same thread plays both roles, sequentially — both contracts hold.
        unsafe {
            for seq in 0..3 {
                ring.push(&rec(seq));
            }
            let mut seen = Vec::new();
            ring.drain(&mut |b| collect_seqs(b, &mut seen));
            assert_eq!(seen, vec![0, 1, 2]);
            assert_eq!(ring.free_len(), 2, "two retired segments should be pooled");

            let live_after_drain = ctx.live_bytes();
            for seq in 3..5 {
                ring.push(&rec(seq)); // two links — both must pop the free list
            }
            assert_eq!(
                ctx.live_bytes(),
                live_after_drain,
                "growth should recycle, not allocate"
            );
            assert_eq!(ring.free_len(), 0);

            let mut seen = Vec::new();
            ring.drain(&mut |b| collect_seqs(b, &mut seen));
            assert_eq!(seen, vec![3, 4]);
        }
        // Behavioral leak check (miri's leak checker is off for these tests):
        // Ring::drop must walk both the live chain and the free list back to
        // a zero balance.
        drop(tr);
        assert_eq!(ctx.live_bytes(), 0, "Ring::drop leaked segments");
    }

    /// (3) Free-list SPSC integrity: heavier producer/consumer traffic so
    /// pops (producer) and pushes (consumer) overlap on the Treiber list.
    /// Loom additionally proves the no-ABA argument: any stale-head reuse
    /// would surface as a race on the segment cells or as data corruption.
    pub(super) fn freelist_spsc() {
        let ctx = leak_ctx(BIG_CAP);
        let tr = TestRing::new(ctx, 8, 2, 1); // cap 2 also exercises D7 free-at-cap
        let ring = tr.get();
        let producer = thread::spawn(move || {
            for seq in 0..6 {
                // SAFETY: unique producer.
                unsafe { ring.push(&rec(seq)) };
            }
        });
        let mut seen = Vec::new();
        for _ in 0..3 {
            // SAFETY: single consumer.
            unsafe { ring.drain(&mut |b| collect_seqs(b, &mut seen)) };
        }
        producer.join().unwrap();
        unsafe { ring.drain(&mut |b| collect_seqs(b, &mut seen)) };
        assert_eq!(seen, (0..6).collect::<Vec<_>>());
        assert!(ring.free_len() <= 2, "free list exceeded its cap");
    }

    /// (4) Lifecycle: producer A pushes and dies (orphan); the consumer
    /// drains to empty and pools; producer B claims and pushes. Every record
    /// must arrive, tagged with the right engine — including under the
    /// claim-vs-drain race, where the consumer may read `engine_id` the
    /// moment B's first commit lands.
    pub(super) fn orphan_pool_claim() {
        let ctx = leak_ctx(BIG_CAP);
        let tr = TestRing::new(ctx, 16, 8, 1);
        let ring = tr.get();

        let mut tagged: Vec<(u64, u64)> = Vec::new();
        let mut sink = |ring: &'static Ring, bytes: &[u8]| {
            // SAFETY: bytes in hand are drain progress (engine_id contract).
            let engine = unsafe { ring.engine_id() };
            let mut seqs = Vec::new();
            collect_seqs(bytes, &mut seqs);
            tagged.extend(seqs.into_iter().map(|s| (engine, s)));
        };
        let drain = |sink: &mut dyn FnMut(&'static Ring, &[u8])| {
            // SAFETY: this thread is the single consumer.
            unsafe { ring.drain(&mut |b| sink(ring, b)) }
        };

        let a = thread::spawn(move || {
            // SAFETY: creator-claimant.
            unsafe {
                ring.push(&rec(0));
                ring.push(&rec(1));
            }
            ring.orphan(); // what the TLS destructor does on thread death
        });
        for _ in 0..2 {
            drain(&mut sink);
        }
        a.join().unwrap();
        assert_eq!(ring.state(), RingState::Orphaned);
        // Consumer: the orphan edge made every pre-death push visible — one
        // drain reaches empty; then pool.
        drain(&mut sink);
        // SAFETY: consumer just drained the orphaned ring to empty.
        unsafe { ring.mark_pooled() };

        let b = thread::spawn(move || {
            assert!(ring.try_claim(2), "pooled ring must be claimable");
            // SAFETY: the claim made this thread the unique producer.
            unsafe {
                ring.push(&rec(2));
                ring.push(&rec(3));
            }
        });
        for _ in 0..2 {
            drain(&mut sink);
        }
        b.join().unwrap();
        drain(&mut sink);

        assert_eq!(tagged, vec![(1, 0), (1, 1), (2, 2), (2, 3)]);
    }

    /// (4b) A pooled ring is claimable exactly once.
    pub(super) fn claim_is_exclusive() {
        let ctx = leak_ctx(BIG_CAP);
        let tr = TestRing::new(ctx, 16, 8, 1);
        let ring = tr.get();
        ring.orphan(); // producer (this thread) dies without pushing
        // SAFETY: single consumer; ring is empty.
        unsafe {
            ring.drain(&mut |_| panic!("empty ring yielded bytes"));
            ring.mark_pooled();
        }
        let c1 = thread::spawn(move || ring.try_claim(2));
        let c2 = thread::spawn(move || ring.try_claim(3));
        let (r1, r2) = (c1.join().unwrap(), c2.join().unwrap());
        assert!(r1 ^ r2, "exactly one claimant must win (got {r1} and {r2})");
    }

    /// (5) Registry: concurrent acquires never hand two threads the same
    /// ring, appends are lossless, and the sweep sees every ring's records.
    pub(super) fn registry_concurrent_acquire() {
        let ctx = leak_ctx(BIG_CAP);
        let reg_ptr = Box::into_raw(Box::new(Registry::new()));
        let reg: &'static Registry = unsafe { &*reg_ptr };

        let t1 = thread::spawn(move || {
            let h = reg.acquire(ctx, 16, 8, 1).expect("test ring capacity");
            // SAFETY: the claiming thread is alive for the whole call (no TLS
            // teardown in flight).
            unsafe { h.push(&rec(101)) };
            std::ptr::from_ref(h.ring()) as usize
        });
        let t2 = thread::spawn(move || {
            let h = reg.acquire(ctx, 16, 8, 2).expect("test ring capacity");
            // SAFETY: the claiming thread is alive for the whole call (no TLS
            // teardown in flight).
            unsafe { h.push(&rec(202)) };
            std::ptr::from_ref(h.ring()) as usize
        });
        let (p1, p2) = (t1.join().unwrap(), t2.join().unwrap());
        assert_ne!(p1, p2, "two live producers ended up sharing a ring");

        let mut tagged: Vec<(u64, u64)> = Vec::new();
        // SAFETY: single consumer thread.
        unsafe {
            reg.sweep(&mut |ring, bytes| {
                let engine = ring.engine_id();
                let mut seqs = Vec::new();
                collect_seqs(bytes, &mut seqs);
                tagged.extend(seqs.into_iter().map(|s| (engine, s)));
            });
        }
        tagged.sort_unstable();
        assert_eq!(tagged, vec![(1, 101), (2, 202)]);

        let mut rings = 0;
        reg.for_each(|_| rings += 1);
        assert_eq!(rings, 2);

        drop(unsafe { Box::from_raw(reg_ptr) });
    }

    /// (5a) Registry traversal preserves ring registration order. Parent
    /// producers register before the workers they spawn; FIFO sweep order is
    /// the bounded-memory causal fast path for cross-ring decoder joins.
    pub(super) fn registry_preserves_registration_order() {
        let ctx = leak_ctx(BIG_CAP);
        let reg = Registry::new();
        let first = reg.acquire(ctx, 16, 8, 1).expect("first test ring");
        let second = reg.acquire(ctx, 16, 8, 2).expect("second test ring");
        unsafe {
            first.push(&rec(101));
            second.push(&rec(202));
        }
        let mut engines = Vec::new();
        unsafe {
            reg.sweep(&mut |ring, _| engines.push(ring.engine_id()));
        }
        assert_eq!(engines, vec![1, 2]);
        drop(reg);
        assert_eq!(ctx.live_bytes(), 0, "Registry::drop leaked segments");
    }

    /// (5b) Registry pooling: a ring pooled after its producer dies is
    /// claimed through `acquire` by the next producer — concurrently with
    /// the consumer's sweeping — instead of allocating fresh.
    pub(super) fn registry_pool_reuse() {
        let ctx = leak_ctx(BIG_CAP);
        let reg_ptr = Box::into_raw(Box::new(Registry::new()));
        let reg: &'static Registry = unsafe { &*reg_ptr };

        let mut tagged: Vec<(u64, u64)> = Vec::new();
        let sweep = |tagged: &mut Vec<(u64, u64)>| {
            // SAFETY: single consumer thread.
            unsafe {
                reg.sweep(&mut |ring, bytes| {
                    let engine = ring.engine_id();
                    let mut seqs = Vec::new();
                    collect_seqs(bytes, &mut seqs);
                    tagged.extend(seqs.into_iter().map(|s| (engine, s)));
                })
            }
        };

        // Producer 1 lives and dies on a spawned thread.
        let p1 = thread::spawn(move || {
            let h = reg.acquire(ctx, 16, 8, 1).expect("test ring capacity");
            // SAFETY: the claiming thread is alive for the whole call (no TLS
            // teardown in flight).
            unsafe { h.push(&rec(7)) };
            h.ring().orphan(); // TLS-destructor stand-in
            std::ptr::from_ref(h.ring()) as usize
        })
        .join()
        .unwrap();
        // Consumer drains the orphan and pools it; a second sweep must skip
        // the pooled ring.
        sweep(&mut tagged);
        sweep(&mut tagged);

        // Producer 2 claims while the consumer keeps sweeping.
        let t = thread::spawn(move || {
            let h = reg.acquire(ctx, 16, 8, 2).expect("test ring capacity");
            // SAFETY: the claiming thread is alive for the whole call (no TLS
            // teardown in flight).
            unsafe { h.push(&rec(8)) };
            std::ptr::from_ref(h.ring()) as usize
        });
        sweep(&mut tagged);
        let p2 = t.join().unwrap();
        sweep(&mut tagged);

        assert_eq!(p1, p2, "the pooled ring should have been reused");
        let mut rings = 0;
        reg.for_each(|_| rings += 1);
        assert_eq!(rings, 1);
        assert_eq!(tagged, vec![(1, 7), (2, 8)]);

        drop(unsafe { Box::from_raw(reg_ptr) });
        assert_eq!(ctx.live_bytes(), 0, "Registry::drop leaked segments");
    }

    /// (4c) `Registry::sweep` racing `orphan()`: the sweep may load `Active`
    /// in the very step the producer dies, or see the orphan mid-walk. Every
    /// such stale observation must be benign — no record is lost and a later
    /// sweep pools the ring.
    pub(super) fn sweep_races_orphan() {
        let ctx = leak_ctx(BIG_CAP);
        let reg_ptr = Box::into_raw(Box::new(Registry::new()));
        let reg: &'static Registry = unsafe { &*reg_ptr };

        let producer = thread::spawn(move || {
            let h = reg.acquire(ctx, 16, 8, 1).expect("test ring capacity");
            // SAFETY: the claiming thread is alive for the whole call (no TLS
            // teardown in flight).
            unsafe {
                h.push(&rec(0));
                h.push(&rec(1));
            }
            h.ring().orphan(); // TLS-destructor stand-in
        });
        let mut seen = Vec::new();
        let sweep = |seen: &mut Vec<u64>| {
            // SAFETY: this thread is the single consumer.
            unsafe { reg.sweep(&mut |_, bytes| collect_seqs(bytes, seen)) };
        };
        for _ in 0..2 {
            sweep(&mut seen); // concurrent with pushes and the orphan store
        }
        producer.join().unwrap();
        // Post-join, a sweep observes Orphaned (or an earlier one already
        // pooled): drain to empty, pool; the next sweep must skip it.
        sweep(&mut seen);
        sweep(&mut seen);
        assert_eq!(seen, vec![0, 1]);
        let mut state = None;
        reg.for_each(|ring| state = Some(ring.state()));
        assert_eq!(state, Some(RingState::Pooled));

        drop(unsafe { Box::from_raw(reg_ptr) });
        assert_eq!(ctx.live_bytes(), 0, "Registry::drop leaked segments");
    }
}

#[test]
fn segment_growth_pressure_rejects_record_without_aborting() {
    let ctx = leak_ctx(segment_footprint(16));
    let ring = TestRing::new(ctx, 16, 0, 1);
    let producer = ring.get();
    unsafe {
        assert!(producer.push(&rec(1)));
        assert!(producer.push(&rec(2)));
        assert!(!producer.push(&rec(3)));
    }
    let mut observed = Vec::new();
    unsafe {
        producer.drain(&mut |bytes| collect_seqs(bytes, &mut observed));
    }
    assert_eq!(observed, [1, 2]);
    assert_eq!(ctx.live_bytes(), segment_footprint(16));
}

#[cfg(baml_loom)]
mod loom_suite {
    use super::scenarios;

    fn model(f: impl Fn() + Send + Sync + 'static) {
        let mut builder = loom::model::Builder::new();
        // Exhaustive search explodes on the bigger scenarios; bound 3 keeps
        // each test in seconds while covering every ≤3-preemption
        // interleaving (loom's recommended operating point).
        builder.preemption_bound = Some(3);
        builder.check(f);
    }

    #[test]
    fn handoff_no_loss() {
        model(scenarios::handoff_no_loss);
    }

    #[test]
    fn recycle_no_stale() {
        model(scenarios::recycle_no_stale);
    }

    #[test]
    fn recycle_reuses_memory() {
        model(scenarios::recycle_reuses_memory);
    }

    #[test]
    fn freelist_spsc() {
        model(scenarios::freelist_spsc);
    }

    #[test]
    fn orphan_pool_claim() {
        model(scenarios::orphan_pool_claim);
    }

    #[test]
    fn claim_is_exclusive() {
        model(scenarios::claim_is_exclusive);
    }

    #[test]
    fn registry_concurrent_acquire() {
        model(scenarios::registry_concurrent_acquire);
    }

    #[test]
    fn registry_preserves_registration_order() {
        model(scenarios::registry_preserves_registration_order);
    }

    #[test]
    fn registry_pool_reuse() {
        model(scenarios::registry_pool_reuse);
    }

    #[test]
    fn sweep_races_orphan() {
        model(scenarios::sweep_races_orphan);
    }
}

/// The same scenarios on real threads. miri executes these (checking the
/// UnsafeCell/raw-pointer discipline); plain `cargo test` runs them many
/// times as a cheap smoke for what loom checks exhaustively.
#[cfg(not(baml_loom))]
mod std_suite {
    use super::scenarios;

    fn many(f: impl Fn()) {
        let iterations = if cfg!(miri) { 3 } else { 200 };
        for _ in 0..iterations {
            f();
        }
    }

    #[test]
    fn handoff_no_loss() {
        many(scenarios::handoff_no_loss);
    }

    #[test]
    fn recycle_no_stale() {
        many(scenarios::recycle_no_stale);
    }

    #[test]
    fn recycle_reuses_memory() {
        many(scenarios::recycle_reuses_memory);
    }

    #[test]
    fn freelist_spsc() {
        many(scenarios::freelist_spsc);
    }

    #[test]
    fn orphan_pool_claim() {
        many(scenarios::orphan_pool_claim);
    }

    #[test]
    fn claim_is_exclusive() {
        many(scenarios::claim_is_exclusive);
    }

    #[test]
    fn registry_concurrent_acquire() {
        many(scenarios::registry_concurrent_acquire);
    }

    #[test]
    fn registry_preserves_registration_order() {
        many(scenarios::registry_preserves_registration_order);
    }

    #[test]
    fn registry_pool_reuse() {
        many(scenarios::registry_pool_reuse);
    }

    #[test]
    fn sweep_races_orphan() {
        many(scenarios::sweep_races_orphan);
    }
}

#[cfg(not(baml_loom))]
mod stress {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use super::{BIG_CAP, leak_ctx, rec};
    use crate::{
        ids::{BexCallId, BexThreadId, FunctionId},
        prof::{
            record::{self, RawRecord},
            registry::Registry,
            ring::RingState,
        },
    };

    /// The 2+1-thread flood (plan §5 PR2 gate): real record framing through
    /// deliberately tiny segments (constant growth + recycling), bursty
    /// producers, and the real wake/park protocol on the consumer. Asserts
    /// `consumed == produced`, exactly and in order, per engine.
    #[test]
    fn flood_with_growth_recycle_and_wake() {
        let (per_producer, seg_bytes) = if cfg!(miri) {
            (150, 128)
        } else {
            (200_000, 256)
        };
        let producers: usize = 2;
        let freelist_cap = 4;

        let ctx = leak_ctx(BIG_CAP);
        let reg_ptr = Box::into_raw(Box::new(Registry::new()));
        let reg: &'static Registry = unsafe { &*reg_ptr };
        let remaining = Arc::new(AtomicUsize::new(producers));

        ctx.wake().register_consumer();

        let handles: Vec<_> = (1..=producers)
            .map(|engine_idx| {
                let remaining = Arc::clone(&remaining);
                std::thread::spawn(move || {
                    let engine = engine_idx as u64;
                    let h = reg
                        .acquire(ctx, seg_bytes, freelist_cap, engine)
                        .expect("test ring capacity");
                    let mut buf = [0u8; record::MAX_RECORD_LEN];
                    for seq in 1u64..=per_producer {
                        let len = RawRecord::CallFunction {
                            flags: 0,
                            thread_id: BexThreadId(engine),
                            call_id: BexCallId(seq),
                            parent_call_id: BexCallId(seq.saturating_sub(1)),
                            function_id: FunctionId(u32::try_from(engine).unwrap()),
                            call_site: None,
                            ts_ticks: seq,
                        }
                        .encode(&mut buf);
                        // SAFETY: the claiming thread is alive for the whole call (no TLS
                        // teardown in flight).
                        unsafe { h.push(&buf[..len]) };
                        // Two bursts with pauses: lets the consumer catch up
                        // (recycle path) and then fall behind again (growth
                        // path).
                        if seq == per_producer / 3 || seq == 2 * per_producer / 3 {
                            std::thread::sleep(Duration::from_millis(1));
                        }
                    }
                    remaining.fetch_sub(1, Ordering::Release);
                })
            })
            .collect();

        // Consumer: sweep with the D4 protocol (park flag, recheck, timeout).
        let mut per_engine: Vec<Vec<u64>> = vec![Vec::new(); producers + 1];
        let mut sink = |ring: &'static crate::prof::ring::Ring, bytes: &[u8]| {
            // SAFETY: bytes in hand are drain progress.
            let engine = unsafe { ring.engine_id() };
            for r in record::iter(bytes) {
                match r.expect("corrupt record in committed range") {
                    RawRecord::CallFunction {
                        thread_id, call_id, ..
                    } => {
                        assert_eq!(
                            thread_id.0, engine,
                            "record demuxed to the wrong engine's ring"
                        );
                        per_engine[usize::try_from(engine).unwrap()].push(call_id.0);
                    }
                    other => panic!("unexpected record {other:?}"),
                }
            }
        };
        loop {
            // SAFETY: this thread is the single consumer.
            let progress = unsafe { reg.sweep(&mut sink) };
            if progress {
                continue;
            }
            if remaining.load(Ordering::Acquire) == 0 {
                // Producers are done (Acquire pairs with their Release), so
                // one empty sweep means the rings are empty.
                if !unsafe { reg.sweep(&mut sink) } {
                    break;
                }
                continue;
            }
            ctx.wake().pre_park();
            // Recheck after raising the flag (shrinks the benign race);
            // the timeout bounds whatever remains of it.
            if unsafe { reg.sweep(&mut sink) } {
                ctx.wake().post_park();
                continue;
            }
            ctx.wake().park(Duration::from_millis(1));
            ctx.wake().post_park();
        }
        for h in handles {
            h.join().unwrap();
        }

        let expected: Vec<u64> = (1..=per_producer).collect();
        for (engine, calls) in per_engine.iter().enumerate().skip(1) {
            assert_eq!(
                *calls, expected,
                "engine {engine}: lost, duplicated, or reordered records"
            );
        }

        // D7 bound: idle memory is the open segment plus at most
        // freelist_cap pooled segments per ring (512 B covers the segment
        // header generously).
        let mut rings = 0;
        reg.for_each(|ring| {
            rings += 1;
            assert!(ring.free_len() <= freelist_cap);
        });
        assert_eq!(rings, producers);
        assert!(
            ctx.live_bytes() <= rings * (freelist_cap + 1) * (seg_bytes + 512),
            "live ring memory did not shrink back after the flood: {} bytes",
            ctx.live_bytes()
        );

        drop(unsafe { Box::from_raw(reg_ptr) });
        assert_eq!(ctx.live_bytes(), 0, "Registry::drop leaked segments");
    }

    /// Wake delivery, not just wake survival: a parked consumer must be
    /// unparked by a producer's segment-fill wake. The producer keeps filling
    /// segments, so the benign D4 lost-wakeup window cannot swallow every
    /// wake — only a broken path (consumer never registered, dead flag, no
    /// unpark call) leaves the consumer asleep for the full long timeout.
    /// The flood test alone cannot catch that: its 1 ms park timeout makes
    /// a broken Wake merely slow, not failing.
    #[test]
    #[cfg(not(miri))] // wall-clock timing under miri is meaningless
    fn unpark_delivery() {
        use std::sync::atomic::AtomicBool;

        let ctx = leak_ctx(BIG_CAP);
        let tr = super::TestRing::new(ctx, 16, 4, 1);
        let ring = tr.get();
        ctx.wake().register_consumer();

        let stop = Arc::new(AtomicBool::new(false));
        let producer = {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                while !stop.load(Ordering::Acquire) {
                    // Two 8-byte records fill a 16-byte segment: every pair
                    // takes the slow path and calls wake_if_parked.
                    // SAFETY: creator-claimant thread, alive throughout.
                    unsafe {
                        ring.push(&rec(0));
                        ring.push(&rec(1));
                    }
                    std::thread::sleep(Duration::from_micros(200));
                }
            })
        };

        let start = std::time::Instant::now();
        ctx.wake().pre_park();
        ctx.wake().park(Duration::from_secs(10));
        ctx.wake().post_park();
        let waited = start.elapsed();

        stop.store(true, Ordering::Release);
        producer.join().unwrap();

        assert!(
            waited < Duration::from_secs(5),
            "consumer slept {waited:?}: segment-fill unpark was never delivered"
        );
    }

    /// Orphan churn: short-lived producer threads in sequence must recycle
    /// one pooled ring through the registry, not grow it.
    #[test]
    fn orphan_churn_reuses_rings() {
        let ctx = leak_ctx(BIG_CAP);
        let reg_ptr = Box::into_raw(Box::new(Registry::new()));
        let reg: &'static Registry = unsafe { &*reg_ptr };

        let rounds = if cfg!(miri) { 3 } else { 16 };
        let mut tagged: Vec<(u64, u64)> = Vec::new();
        for round in 1..=rounds {
            std::thread::spawn(move || {
                let h = reg.acquire(ctx, 64, 4, round).expect("test ring capacity");
                // SAFETY: the claiming thread is alive for the whole call (no TLS
                // teardown in flight).
                unsafe { h.push(&rec(round)) };
                h.ring().orphan(); // TLS-destructor stand-in
            })
            .join()
            .unwrap();
            // SAFETY: single consumer thread.
            unsafe {
                reg.sweep(&mut |ring, bytes| {
                    let engine = ring.engine_id();
                    let mut seqs = Vec::new();
                    super::collect_seqs(bytes, &mut seqs);
                    tagged.extend(seqs.into_iter().map(|s| (engine, s)));
                });
            }
        }

        let mut rings = 0;
        reg.for_each(|_| rings += 1);
        assert_eq!(rings, 1, "churned threads must reuse the pooled ring");
        assert_eq!(tagged, (1..=rounds).map(|r| (r, r)).collect::<Vec<_>>());

        drop(unsafe { Box::from_raw(reg_ptr) });
    }

    /// The global path end-to-end: `ring_for_engine` + real TLS-destructor
    /// orphaning + pooled reuse by the next thread. Uses the process-global
    /// registry, so it asserts only on its own rings (by address).
    #[test]
    fn global_tls_orphan_and_reuse() {
        use crate::prof::registry::{global_registry, ring_for_engine};

        const ENGINE: u64 = 0xBEEF_0001;

        // This test is its own consumer of the PROCESS-GLOBAL registry, so
        // the real `bex-prof-consumer` must never spawn (two consumers on
        // one ring is the exact SPSC violation the suite exists to rule
        // out). Profiling is default-on (follow-up 16): latch the global
        // config off before `ring_for_engine` reads it. This is the only
        // lib test touching the global path; the assert keeps that true.
        // SAFETY: single-threaded at this point in the test; the config is
        // latched once immediately below.
        unsafe { std::env::set_var(crate::prof::config::ENV_PROFILE, "0") };
        assert!(
            !crate::prof::ProfConfig::global().enabled,
            "another test latched the global ProfConfig with profiling \
             enabled — the global-registry tests need it off"
        );

        let ptr1 = std::thread::spawn(|| {
            let h = ring_for_engine(ENGINE).expect("test ring capacity");
            // SAFETY: the claiming thread is alive for the whole call (no TLS
            // teardown in flight).
            unsafe { h.push(&rec(1)) };
            std::ptr::from_ref(h.ring()) as usize
        })
        .join()
        .unwrap();

        // join() returns after the thread ran its TLS destructors, so the
        // orphan store is visible.
        let mut state = None;
        global_registry().for_each(|ring| {
            if std::ptr::from_ref(ring) as usize == ptr1 {
                state = Some(ring.state());
            }
        });
        assert_eq!(state, Some(RingState::Orphaned));

        // Act as the consumer: drain + pool (only this test sweeps the
        // global registry; the other suites use private registries).
        let mut seqs = Vec::new();
        unsafe {
            global_registry().sweep(&mut |ring, bytes| {
                if std::ptr::from_ref(ring) as usize == ptr1 {
                    super::collect_seqs(bytes, &mut seqs);
                }
            });
        }
        assert_eq!(seqs, vec![1]);

        // The next thread asking for any engine must claim the pooled ring.
        let ptr2 = std::thread::spawn(|| {
            let h = ring_for_engine(ENGINE + 1).expect("test ring capacity");
            // SAFETY: the claiming thread is alive for the whole call (no TLS
            // teardown in flight).
            unsafe { h.push(&rec(2)) };
            std::ptr::from_ref(h.ring()) as usize
        })
        .join()
        .unwrap();
        assert_eq!(ptr1, ptr2, "pooled ring was not reused by the next thread");

        let mut seqs = Vec::new();
        unsafe {
            global_registry().sweep(&mut |ring, bytes| {
                if std::ptr::from_ref(ring) as usize == ptr1 {
                    super::collect_seqs(bytes, &mut seqs);
                }
            });
        }
        assert_eq!(seqs, vec![2]);
    }
}
