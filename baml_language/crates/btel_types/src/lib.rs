//! Transport-independent types shared by BAML telemetry producers.
//!
//! This crate contains identity, time, and invocation vocabulary only. It does
//! not contain buffering, collection, decoding, storage, or publication.

#![allow(unsafe_code)]
#![allow(
    clippy::inline_always,
    reason = "these wrappers and clock/ID primitives are measured producer hot-path operations"
)]

use std::{
    cell::Cell,
    mem::size_of,
    num::NonZeroU64,
    sync::atomic::{AtomicU16, AtomicU64, Ordering},
};

const ID_RANGE_SIZE: u64 = 4096;

/// Identity of an individually identified telemetry graph node.
///
/// Threads, entry-selected spans, and late-promoted spans share this namespace.
/// The private nonzero representation keeps `Option<TelemetryId>` at eight
/// bytes while preventing runtime code from manufacturing arbitrary IDs.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TelemetryId(NonZeroU64);

const _: () = assert!(size_of::<TelemetryId>() == 8);
const _: () = assert!(size_of::<Option<TelemetryId>>() == 8);

impl TelemetryId {
    #[inline(always)]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Index into an engine-owned table of immutable, resolved policies.
///
/// The atomic lives once per function, rather than once per invocation. Policy
/// changes are intentionally observed by calls already in flight when they
/// complete.
#[repr(transparent)]
pub struct TelemetryPolicyId(AtomicU16);

impl TelemetryPolicyId {
    pub const NONE: u16 = 0;

    #[inline(always)]
    pub const fn none() -> Self {
        Self(AtomicU16::new(Self::NONE))
    }

    #[inline(always)]
    pub fn load(&self) -> u16 {
        self.0.load(Ordering::Acquire)
    }

    #[inline(always)]
    pub fn store(&self, policy_id: u16) {
        self.0.store(policy_id, Ordering::Release);
    }
}

impl Default for TelemetryPolicyId {
    fn default() -> Self {
        Self::none()
    }
}

impl Clone for TelemetryPolicyId {
    fn clone(&self) -> Self {
        Self(AtomicU16::new(self.load()))
    }
}

impl std::fmt::Debug for TelemetryPolicyId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("TelemetryPolicyId")
            .field(&self.load())
            .finish()
    }
}

impl PartialEq for TelemetryPolicyId {
    fn eq(&self, other: &Self) -> bool {
        self.load() == other.load()
    }
}

impl Eq for TelemetryPolicyId {}

const _: () = assert!(size_of::<TelemetryPolicyId>() == 2);

#[derive(Clone, Copy)]
struct IdRange {
    next: u64,
    end: u64,
}

thread_local! {
    static TELEMETRY_IDS: Cell<IdRange> = const { Cell::new(IdRange { next: 0, end: 0 }) };
}

static NEXT_ID_RANGE: AtomicU64 = AtomicU64::new(1);

/// Allocate an ID from the current worker thread's reserved range.
#[inline(always)]
pub fn allocate_telemetry_id() -> TelemetryId {
    TELEMETRY_IDS.with(|slot| {
        let mut range = slot.get();
        if range.next == range.end {
            return refill_and_allocate(slot);
        }
        let raw = range.next;
        range.next += 1;
        slot.set(range);
        debug_assert_ne!(raw, 0);
        // SAFETY: range reservation excludes zero and checks overflow.
        TelemetryId(unsafe { NonZeroU64::new_unchecked(raw) })
    })
}

#[cold]
#[inline(never)]
fn refill_and_allocate(slot: &Cell<IdRange>) -> TelemetryId {
    let mut start = NEXT_ID_RANGE.load(Ordering::Relaxed);
    loop {
        let end = start
            .checked_add(ID_RANGE_SIZE)
            .expect("telemetry identity space exhausted");
        match NEXT_ID_RANGE.compare_exchange_weak(start, end, Ordering::Relaxed, Ordering::Relaxed)
        {
            Ok(_) => {
                debug_assert_ne!(start, 0);
                slot.set(IdRange {
                    next: start + 1,
                    end,
                });
                // SAFETY: the first reserved range begins at one and checked
                // addition prevents a wrapping range.
                return TelemetryId(unsafe { NonZeroU64::new_unchecked(start) });
            }
            Err(observed) => start = observed,
        }
    }
}

/// Stable aggregation identity for repeated calls along one call path.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct CallPathId(u32);

impl CallPathId {
    pub const ROOT: Self = Self(0);

    #[inline(always)]
    pub const fn new_non_root(raw: u32) -> Option<Self> {
        if raw == 0 { None } else { Some(Self(raw)) }
    }

    #[inline(always)]
    pub const fn get(self) -> u32 {
        self.0
    }
}

const _: () = assert!(size_of::<CallPathId>() == 4);

/// A raw reading from the producer clock. The unit is supplied by run metadata.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct ClockInstant(u64);

/// An elapsed amount in producer clock ticks.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ClockDuration(u64);

/// Invocation-local accumulated self-await time.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct AwaitDuration(ClockDuration);

const _: () = assert!(size_of::<ClockInstant>() == 8);
const _: () = assert!(size_of::<ClockDuration>() == 8);
const _: () = assert!(size_of::<AwaitDuration>() == 8);

impl ClockInstant {
    #[inline(always)]
    pub const fn from_ticks(ticks: u64) -> Self {
        Self(ticks)
    }

    #[inline(always)]
    pub const fn elapsed_until(self, end: Self) -> ClockDuration {
        ClockDuration(end.0.saturating_sub(self.0))
    }

    #[inline(always)]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl ClockDuration {
    pub const ZERO: Self = Self(0);

    #[inline(always)]
    pub const fn from_ticks(ticks: u64) -> Self {
        Self(ticks)
    }

    #[inline(always)]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl AwaitDuration {
    pub const ZERO: Self = Self(ClockDuration::ZERO);

    #[inline(always)]
    #[must_use]
    pub const fn saturating_add(self, elapsed: ClockDuration) -> Self {
        Self(ClockDuration(self.0.0.saturating_add(elapsed.0)))
    }

    #[inline(always)]
    pub const fn get(self) -> ClockDuration {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvocationMode {
    Hidden,
    Timing,
    Span,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvocationOutcome {
    Ok,
    Errored,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CallPathEdge {
    Synchronous,
    Spawn,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocator_ids_are_unique_across_workers_and_range_refills() {
        let workers: Vec<_> = (0..4)
            .map(|_| {
                std::thread::spawn(|| {
                    (0..=ID_RANGE_SIZE * 2)
                        .map(|_| allocate_telemetry_id())
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let mut ids = std::collections::HashSet::new();
        for worker in workers {
            for id in worker.join().unwrap() {
                assert!(ids.insert(id), "telemetry ID was allocated twice: {id:?}");
            }
        }
    }
}
