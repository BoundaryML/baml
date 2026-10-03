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
/// Includes producer-owned, queued, processor-owned and idle descriptors.
pub const MIN_SNAPSHOT_SLOTS: usize = 128;

/// Hash and copy mutable bytes in cache-sized batches during capture.
pub const COPY_HASH_BATCH_BYTES: usize = 16 * 1024;

/// Where captures are cut into CAS blobs. These decide blob IDs, never whether
/// a blob can be read: changing them costs dedup against blobs cut the old
/// way, not a format bump.
///
/// A value whose encoded size reaches this gets a blob of its own, unless
/// the rest of the capture references a part of it.
pub const SPLIT_UNIT_BYTES: u64 = 64 * 1024;
/// A string or bigint value, or a `uint8array`, whose content (a bigint's
/// encoded limbs) reaches this gets a blob of its own, unless it is the
/// captured value itself. Map keys and names are always written in place.
pub const SPLIT_LEAF_BYTES: u64 = 16 * 1024;

/// The one size a capture is cut for. A string, a bigint (its encoded limbs),
/// a `uint8array` or a media value's content over this is captured as
/// truncated: it cannot be split, and whatever stores or sends it holds it
/// whole. A map entry whose key is over it is left out. Everything else is
/// captured however large it is.
pub const MAX_LEAF_BYTES: usize = 1 << 30;

/// Bounded processor-local whole-snapshot combining window, not a delivery ledger.
pub const RECENT_CAPTURE_IDS: usize = 4096;

/// CAS blob envelope. Change with the binary codec, never as a tuning knob.
pub const BLOB_MAGIC: [u8; 8] = *b"BTELCAS\0";
pub const BLOB_VERSION: u32 = 3;
