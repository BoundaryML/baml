use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, Default)]
pub struct BexStrAllocationStats {
    pub flat_nodes_created: u64,
    pub flat_nodes_dropped: u64,
    pub flat_nodes_live: u64,
    pub flat_payloads_created: u64,
    pub flat_payloads_dropped: u64,
    pub flat_payloads_live: u64,
    pub flat_payload_bytes_created: u64,
    pub flat_payload_bytes_dropped: u64,
    pub flat_payload_bytes_live: u64,
    pub concat_nodes_created: u64,
    pub concat_nodes_dropped: u64,
    pub concat_nodes_live: u64,
    pub flatten_calls: u64,
}

static FLAT_NODES_CREATED: AtomicU64 = AtomicU64::new(0);
static FLAT_NODES_DROPPED: AtomicU64 = AtomicU64::new(0);
static FLAT_NODES_LIVE: AtomicU64 = AtomicU64::new(0);
static FLAT_PAYLOADS_CREATED: AtomicU64 = AtomicU64::new(0);
static FLAT_PAYLOADS_DROPPED: AtomicU64 = AtomicU64::new(0);
static FLAT_PAYLOADS_LIVE: AtomicU64 = AtomicU64::new(0);
static FLAT_PAYLOAD_BYTES_CREATED: AtomicU64 = AtomicU64::new(0);
static FLAT_PAYLOAD_BYTES_DROPPED: AtomicU64 = AtomicU64::new(0);
static FLAT_PAYLOAD_BYTES_LIVE: AtomicU64 = AtomicU64::new(0);
static CONCAT_NODES_CREATED: AtomicU64 = AtomicU64::new(0);
static CONCAT_NODES_DROPPED: AtomicU64 = AtomicU64::new(0);
static CONCAT_NODES_LIVE: AtomicU64 = AtomicU64::new(0);
static FLATTEN_CALLS: AtomicU64 = AtomicU64::new(0);

pub(crate) fn record_flat_created(payload_bytes: usize) {
    FLAT_NODES_CREATED.fetch_add(1, Ordering::Relaxed);
    FLAT_NODES_LIVE.fetch_add(1, Ordering::Relaxed);
    if payload_bytes > 0 {
        let payload_bytes = payload_bytes as u64;
        FLAT_PAYLOADS_CREATED.fetch_add(1, Ordering::Relaxed);
        FLAT_PAYLOADS_LIVE.fetch_add(1, Ordering::Relaxed);
        FLAT_PAYLOAD_BYTES_CREATED.fetch_add(payload_bytes, Ordering::Relaxed);
        FLAT_PAYLOAD_BYTES_LIVE.fetch_add(payload_bytes, Ordering::Relaxed);
    }
}

pub(crate) fn record_flat_dropped(payload_bytes: usize) {
    FLAT_NODES_DROPPED.fetch_add(1, Ordering::Relaxed);
    FLAT_NODES_LIVE.fetch_sub(1, Ordering::Relaxed);
    if payload_bytes > 0 {
        let payload_bytes = payload_bytes as u64;
        FLAT_PAYLOADS_DROPPED.fetch_add(1, Ordering::Relaxed);
        FLAT_PAYLOADS_LIVE.fetch_sub(1, Ordering::Relaxed);
        FLAT_PAYLOAD_BYTES_DROPPED.fetch_add(payload_bytes, Ordering::Relaxed);
        FLAT_PAYLOAD_BYTES_LIVE.fetch_sub(payload_bytes, Ordering::Relaxed);
    }
}

pub(crate) fn record_concat_created() {
    CONCAT_NODES_CREATED.fetch_add(1, Ordering::Relaxed);
    CONCAT_NODES_LIVE.fetch_add(1, Ordering::Relaxed);
}

pub(crate) fn record_concat_dropped() {
    CONCAT_NODES_DROPPED.fetch_add(1, Ordering::Relaxed);
    CONCAT_NODES_LIVE.fetch_sub(1, Ordering::Relaxed);
}

pub(crate) fn record_flatten_call() {
    FLATTEN_CALLS.fetch_add(1, Ordering::Relaxed);
}

pub fn allocation_stats() -> BexStrAllocationStats {
    BexStrAllocationStats {
        flat_nodes_created: FLAT_NODES_CREATED.load(Ordering::Relaxed),
        flat_nodes_dropped: FLAT_NODES_DROPPED.load(Ordering::Relaxed),
        flat_nodes_live: FLAT_NODES_LIVE.load(Ordering::Relaxed),
        flat_payloads_created: FLAT_PAYLOADS_CREATED.load(Ordering::Relaxed),
        flat_payloads_dropped: FLAT_PAYLOADS_DROPPED.load(Ordering::Relaxed),
        flat_payloads_live: FLAT_PAYLOADS_LIVE.load(Ordering::Relaxed),
        flat_payload_bytes_created: FLAT_PAYLOAD_BYTES_CREATED.load(Ordering::Relaxed),
        flat_payload_bytes_dropped: FLAT_PAYLOAD_BYTES_DROPPED.load(Ordering::Relaxed),
        flat_payload_bytes_live: FLAT_PAYLOAD_BYTES_LIVE.load(Ordering::Relaxed),
        concat_nodes_created: CONCAT_NODES_CREATED.load(Ordering::Relaxed),
        concat_nodes_dropped: CONCAT_NODES_DROPPED.load(Ordering::Relaxed),
        concat_nodes_live: CONCAT_NODES_LIVE.load(Ordering::Relaxed),
        flatten_calls: FLATTEN_CALLS.load(Ordering::Relaxed),
    }
}
