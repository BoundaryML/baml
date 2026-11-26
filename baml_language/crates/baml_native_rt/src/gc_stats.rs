//! What `BAML_GC_STATS=1` reports: one `baml-gc-stats {json}` line on
//! stderr when the program ends.
//!
//! Off, the cost is one load and one predictable branch per collection: a
//! collection asks [`start`] for a clock, which is `None` unless the
//! variable was set when the program started.

use std::{
    fmt::Write as _,
    io::Write as _,
    sync::{
        Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

/// Set in the environment, to anything but `0`, to have the program report
/// its collections.
pub(crate) const VARIABLE: &str = "BAML_GC_STATS";
/// The most pauses kept one by one; the totals count every pause.
pub(crate) const MOST_PAUSES: usize = 20_000;

static ENABLED: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
pub(crate) struct Stats {
    pub(crate) collections: u64,
    pub(crate) pause_total: u64,
    pub(crate) pause_max: u64,
    pub(crate) pauses: Vec<u64>,
    pub(crate) bytes_copied: u64,
    /// The most bytes the spaces held from the allocator at once: both
    /// while one is copied into the other.
    pub(crate) heap_peak: usize,
    /// Rust values collections found dead and dropped, and the time
    /// dropping them took, which no pause includes.
    pub(crate) owned_dropped: u64,
    pub(crate) owned_drop_ns: u64,
    /// Time spent dropping values inside the pause being measured, which
    /// [`pause`] takes out of it.
    drop_in_pause: u64,
}

static STATS: Mutex<Stats> = Mutex::new(Stats {
    collections: 0,
    pause_total: 0,
    pause_max: 0,
    pauses: Vec::new(),
    bytes_copied: 0,
    heap_peak: 0,
    owned_dropped: 0,
    owned_drop_ns: 0,
    drop_in_pause: 0,
});

pub(crate) fn stats() -> MutexGuard<'static, Stats> {
    STATS.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(crate) fn nanos_since(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

/// Read the variable: once, as the program starts.
#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "the program's entry calls it, which unit tests leave out"
    )
)]
pub(crate) fn init() {
    if std::env::var_os(VARIABLE).is_some_and(|value| value != "0") {
        ENABLED.store(true, Ordering::Relaxed);
    }
}

/// Turn reporting on or off, as the variable would have.
#[cfg(test)]
pub(crate) fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// Whether collections are being reported.
#[inline]
pub(crate) fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// The clock a pause starts on, when reporting.
#[inline]
pub(crate) fn start() -> Option<Instant> {
    if enabled() {
        Some(Instant::now())
    } else {
        None
    }
}

/// A pause that started at `started` is over. Time spent dropping dead
/// Rust values inside it is not counted: see [`dropped`].
#[cold]
pub(crate) fn pause(started: Instant) {
    let elapsed = nanos_since(started);
    let mut stats = stats();
    let nanos = elapsed.saturating_sub(std::mem::take(&mut stats.drop_in_pause));
    stats.collections += 1;
    stats.pause_total += nanos;
    stats.pause_max = stats.pause_max.max(nanos);
    if stats.pauses.len() < MOST_PAUSES {
        stats.pauses.push(nanos);
    }
}

/// A collection found `count` dead Rust values and dropped them, taking
/// `nanos`, inside the pause being measured. The time is left out of the
/// pause and reported apart: it is the values' destructors, not the
/// collector, and it is not bounded by the heap.
#[cold]
pub(crate) fn dropped(nanos: u64, count: usize) {
    let mut stats = stats();
    stats.owned_dropped += count as u64;
    stats.owned_drop_ns += nanos;
    stats.drop_in_pause += nanos;
}

/// Bytes a copying pass moved into its space.
#[cold]
pub(crate) fn copied(bytes: usize) {
    stats().bytes_copied += bytes as u64;
}

/// The spaces hold `bytes` from the allocator now.
#[cold]
pub(crate) fn heap_bytes(bytes: usize) {
    let mut stats = stats();
    stats.heap_peak = stats.heap_peak.max(bytes);
}

/// The line, from what has been counted. `owned_at_exit` is how many Rust
/// values the heap still held when the program ended, dead ones no
/// collection had found yet included, all dropped then.
pub(crate) fn line(stats: &Stats, owned_at_exit: usize) -> String {
    let mut line = format!(
        "baml-gc-stats {{\"collections\":{},\"pause_ns_total\":{},\"pause_ns_max\":{},\
         \"bytes_copied\":{},\"heap_capacity_peak\":{},\"owned_dropped_by_collections\":{},\
         \"owned_drop_ns_total\":{},\"owned_dropped_at_exit\":{owned_at_exit},\"pause_ns\":[",
        stats.collections,
        stats.pause_total,
        stats.pause_max,
        stats.bytes_copied,
        stats.heap_peak,
        stats.owned_dropped,
        stats.owned_drop_ns,
    );
    for (index, pause) in stats.pauses.iter().enumerate() {
        if index != 0 {
            line.push(',');
        }
        let _ = write!(line, "{pause}");
    }
    line.push_str("]}");
    line
}

/// Write the line on stderr, when reporting.
pub(crate) fn report(owned_at_exit: usize) {
    if !enabled() {
        return;
    }
    let line = line(&stats(), owned_at_exit);
    let _ = writeln!(std::io::stderr().lock(), "{line}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_is_one_json_object_after_its_prefix() {
        let stats = Stats {
            collections: 3,
            pause_total: 60,
            pause_max: 30,
            pauses: vec![10, 20, 30],
            bytes_copied: 4096,
            heap_peak: 16 << 20,
            owned_dropped: 2,
            owned_drop_ns: 7,
            drop_in_pause: 0,
        };
        assert_eq!(
            line(&stats, 1),
            "baml-gc-stats {\"collections\":3,\"pause_ns_total\":60,\"pause_ns_max\":30,\
             \"bytes_copied\":4096,\"heap_capacity_peak\":16777216,\
             \"owned_dropped_by_collections\":2,\"owned_drop_ns_total\":7,\
             \"owned_dropped_at_exit\":1,\"pause_ns\":[10,20,30]}"
        );
        assert!(line(&Stats::default(), 0).ends_with("\"pause_ns\":[]}"));
    }
}
