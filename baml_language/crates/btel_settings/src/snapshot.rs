//! Capture settings: pool tuning, which never limits an active capture, and
//! how captures become CAS blobs.
/// Initial arena capacities; geometric growth handles larger captures.
pub const INITIAL_OBJECTS: usize = 8;
pub const INITIAL_ENTRIES: usize = 8;
pub const INITIAL_BYTES: usize = 0;
pub const INITIAL_VALUES: usize = 4;
/// Total idle structural bytes retained by one runtime's pool.
pub const RETENTION_BUDGET_BYTES: usize = 8 * 1024 * 1024;
/// Discard unusually large returned allocations, regardless of remaining budget.
pub const MAX_RETAINED_ALLOCATION_BYTES: usize = 256 * 1024;

/// **Tune first under backpressure.** Structural capture memory one runtime's
/// producers may hold before a capture waits for the processor: the captures
/// being built, and those in records the processor has not yet taken. What
/// the processor, publisher and delivery hold is bounded by their own budgets,
/// not this one. A single capture is never refused for its size.
pub const CAPTURE_BUDGET_BASE_BYTES: usize = 64 * 1024 * 1024;
/// **Tune with the transport.** The budget grows with admitted producers as
/// the chunk lanes do, so a wide host pool is not starved by its own width.
pub const CAPTURE_BUDGET_BYTES_PER_PRODUCER: usize = 1024 * 1024;

/// The capture budget of a runtime admitting `producers` OS threads.
pub fn capture_budget_bytes(producers: usize) -> usize {
    producers
        .saturating_mul(CAPTURE_BUDGET_BYTES_PER_PRODUCER)
        .saturating_add(CAPTURE_BUDGET_BASE_BYTES)
}
const _: () = assert!(CAPTURE_BUDGET_BASE_BYTES > 0);

/// **Backstop; tune first under sustained backpressure.** Probes a producer
/// makes for the budget to open, when it is full but not yet exceeded,
/// before taking storage beyond it. One probe is one spin.
pub const OVERAGE_PROBES_BASE: usize = 256;
/// **Backstop.** Bytes of overage per step of the probe curve; see
/// [`crate::overage`].
pub const OVERAGE_UNIT_BYTES: usize = 1024 * 1024;

/// Probes before a producer takes storage beyond the budget, with the
/// producers already `overage_bytes` over it.
pub fn overage_probes(overage_bytes: usize) -> usize {
    crate::overage::probes(OVERAGE_PROBES_BASE, overage_bytes / OVERAGE_UNIT_BYTES)
}
const _: () = assert!(OVERAGE_PROBES_BASE > 0 && OVERAGE_UNIT_BYTES > 0);

/// Hash and copy mutable bytes in cache-sized batches during capture.
pub const COPY_HASH_BATCH_BYTES: usize = 16 * 1024;
/// Hash input a blob ID gathers before it streams the rest: a small blob is
/// hashed in one call. Speed and scratch memory only; the ID is the same.
pub const GATHERED_HASH_BYTES: usize = 64 * 1024;

/// Where captures are cut into CAS blobs. These decide blob IDs, never whether
/// a blob can be read: changing them costs dedup against blobs cut the old
/// way, not a format bump.
///
/// A value whose encoded size reaches this gets a blob of its own, unless
/// the rest of the capture references a part of it.
pub const SPLIT_UNIT_BYTES: u64 = 64 * 1024;
/// A string or bigint value, or a `uint8array`, whose content (a bigint's
/// encoded limbs) reaches this gets a blob of its own, unless it is the
/// captured value itself. Names are always written in place.
pub const SPLIT_LEAF_BYTES: u64 = 16 * 1024;

/// The one size a capture is cut for. A string, a bigint (its encoded limbs),
/// a `uint8array` or a media value's content over this is captured as
/// truncated: it cannot be split, and whatever stores or sends it holds it
/// whole. An instance field whose name is over it is left out. Everything else is
/// captured however large it is.
pub const MAX_LEAF_BYTES: usize = 1 << 30;

/// Bounded processor-local whole-snapshot combining window, not a delivery ledger.
pub const RECENT_CAPTURE_IDS: usize = 4096;

/// Definition groups one stream of captures remembers carrying. Past this the
/// stream forgets them all and carries each again once.
pub const CARRIED_GROUPS: usize = 4096;

/// CAS blob envelope. Change with the binary codec, never as a tuning knob.
///
/// One narrowing was made in place, under no version of its own: a blob no
/// longer carries the type family's retired `Void` (discriminant 22; `void`
/// is a spelling of `null`). Nothing writes it, so every blob written since
/// reads under either codec, but a blob of either readable version written
/// before that and naming a `void` type no longer decodes.
pub const BLOB_MAGIC: [u8; 8] = *b"BTELCAS\0";
pub const BLOB_VERSION: u32 = 5;
/// Supported read formats, newest first, for decoding and on-disk lookup.
pub const READABLE_BLOB_VERSIONS: &[u32] = &[BLOB_VERSION, 4, 3];
