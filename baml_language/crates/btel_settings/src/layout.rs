//! Shared cache-line separation and checked storage budgets.
// Rust requires a literal in repr(align); this wrapper is the single definition.
#[repr(align(128))]
pub struct Padded<T>(pub T);
/// **Derived from Padded.** Changing padding requires contention and footprint measurements;
/// edit the wrapper alignment, not this value.
pub const CACHE_ALIGNMENT: usize = align_of::<Padded<()>>();
/// **Checked layout.** Change the frame representation first; editing this assertion target
/// does not reduce runtime storage.
pub const FRAME_TELEMETRY_BYTES: usize = 32;
/// **Checked layout.** Bound for compact timing storage; change record representation before
/// adjusting.
pub const TIMING_RECORD_MAX_BYTES: usize = 32;
/// **Checked layout.** Bound for the checked span payload shape; changing the bound does not
/// compress the payload.
pub const SPAN_RECORD_MAX_BYTES: usize = 56;
/// **Checked layout.** Determines combining-cache slot footprint; review together with its
/// representation and measurements.
pub const AGGREGATE_DELTA_BYTES: usize = 32;
/// **Checked layout.** Second-level totals footprint used in memory accounting.
pub const AGGREGATE_TOTALS_BYTES: usize = 24;
/// **Checked layout.** Per-producer context footprint; review worker-capacity memory when
/// changing.
pub const DECODER_CONTEXT_BYTES: usize = 16;
