//! Capture-only pool tuning. These settings never limit an active capture.
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

/// Bounded processor-local whole-snapshot combining window, not a delivery ledger.
pub const RECENT_CAPTURE_IDS: usize = 4096;

/// CAS blob envelope. Change with the binary codec, never as a tuning knob.
pub const BLOB_MAGIC: [u8; 8] = *b"BTELCAS\0";
pub const BLOB_VERSION: u32 = 2;
