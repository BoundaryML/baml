//! Identity allocation and weak function-lookup storage.
/// **Measure first if refills are material.** Larger ranges amortize rare global atomic
/// refills but waste more IDs on worker retirement; TLS access remains.
pub const ID_RANGE_SIZE: u64 = 4096;
/// **Measure first under registration contention.** More shards consume more padded lock/map
/// storage. Existing call-path cache hits do not access these maps.
pub const SHARD_BITS: u32 = 6;
/// **Derived.** Change [`SHARD_BITS`]; do not set the count independently.
pub const SHARD_COUNT: usize = 1 << SHARD_BITS;
/// **Algorithm choice; measure first.** High product bits distribute sequential/periodic IDs.
/// Require distribution and workload measurements before replacing it.
pub const HASH_MULTIPLIER: u64 = 0x9e37_79b9_7f4a_7c15;
/// **Measure first on dynamic-function churn.** Larger divisor waits for lower occupancy
/// before shrinking; balances retained memory against reallocations.
pub const SHRINK_OCCUPANCY_DIVISOR: usize = 4;
/// **Measure first with shrink threshold.** Spare capacity retained relative to live weak
/// registrations after GC.
pub const RETAINED_CAPACITY_MULTIPLIER: usize = 2;
const _: () = assert!(SHARD_BITS > 0 && SHARD_BITS < usize::BITS);
