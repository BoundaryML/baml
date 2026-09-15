use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicU64, Ordering},
};

const BUCKET_UPPER_BOUNDS: [usize; 9] = [16, 32, 64, 128, 256, 512, 1024, 4096, usize::MAX];
const BUCKET_NAMES: [&str; 9] = [
    "rust_live_blocks_le_16",
    "rust_live_blocks_17_32",
    "rust_live_blocks_33_64",
    "rust_live_blocks_65_128",
    "rust_live_blocks_129_256",
    "rust_live_blocks_257_512",
    "rust_live_blocks_513_1024",
    "rust_live_blocks_1025_4096",
    "rust_live_blocks_gt_4096",
];

static ALLOC_CALLS: AtomicU64 = AtomicU64::new(0);
static ALLOC_ZEROED_CALLS: AtomicU64 = AtomicU64::new(0);
static REALLOC_CALLS: AtomicU64 = AtomicU64::new(0);
static DEALLOC_CALLS: AtomicU64 = AtomicU64::new(0);
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static DEALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static LIVE_BLOCKS: AtomicU64 = AtomicU64::new(0);
static LIVE_BYTES: AtomicU64 = AtomicU64::new(0);
static PEAK_LIVE_BYTES: AtomicU64 = AtomicU64::new(0);
static LIVE_BUCKET_BLOCKS: [AtomicU64; 9] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];

pub struct TrackingAllocator;

#[global_allocator]
static GLOBAL_ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn bucket(size: usize) -> usize {
    BUCKET_UPPER_BOUNDS
        .iter()
        .position(|upper| size <= *upper)
        .expect("last allocation bucket is unbounded")
}

fn update_peak(value: u64) {
    let mut peak = PEAK_LIVE_BYTES.load(Ordering::Relaxed);
    while value > peak {
        match PEAK_LIVE_BYTES.compare_exchange_weak(
            peak,
            value,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(current) => peak = current,
        }
    }
}

fn record_alloc(size: usize) {
    let size = size as u64;
    ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
    ALLOCATED_BYTES.fetch_add(size, Ordering::Relaxed);
    LIVE_BLOCKS.fetch_add(1, Ordering::Relaxed);
    let live_bytes = LIVE_BYTES.fetch_add(size, Ordering::Relaxed) + size;
    LIVE_BUCKET_BLOCKS[bucket(size as usize)].fetch_add(1, Ordering::Relaxed);
    update_peak(live_bytes);
}

fn record_dealloc(size: usize) {
    let size = size as u64;
    DEALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
    DEALLOCATED_BYTES.fetch_add(size, Ordering::Relaxed);
    LIVE_BLOCKS.fetch_sub(1, Ordering::Relaxed);
    LIVE_BYTES.fetch_sub(size, Ordering::Relaxed);
    LIVE_BUCKET_BLOCKS[bucket(size as usize)].fetch_sub(1, Ordering::Relaxed);
}

fn record_realloc(old_size: usize, new_size: usize) {
    REALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
    ALLOCATED_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
    DEALLOCATED_BYTES.fetch_add(old_size as u64, Ordering::Relaxed);
    LIVE_BUCKET_BLOCKS[bucket(old_size)].fetch_sub(1, Ordering::Relaxed);
    LIVE_BUCKET_BLOCKS[bucket(new_size)].fetch_add(1, Ordering::Relaxed);
    let live_bytes = if new_size >= old_size {
        LIVE_BYTES.fetch_add((new_size - old_size) as u64, Ordering::Relaxed)
            + (new_size - old_size) as u64
    } else {
        LIVE_BYTES.fetch_sub((old_size - new_size) as u64, Ordering::Relaxed)
            - (old_size - new_size) as u64
    };
    update_peak(live_bytes);
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code, reason = "calls the macOS malloc zone statistics API")]
fn process_allocator_stats() -> [u64; 4] {
    #[repr(C)]
    #[derive(Default)]
    struct MallocStatistics {
        blocks_in_use: std::os::raw::c_uint,
        size_in_use: usize,
        max_size_in_use: usize,
        size_allocated: usize,
    }

    unsafe extern "C" {
        fn malloc_zone_statistics(zone: *mut std::ffi::c_void, stats: *mut MallocStatistics);
    }

    let mut stats = MallocStatistics::default();
    // SAFETY: Apple's malloc API accepts a null zone to aggregate every zone
    // in the current process and initializes the caller-owned output struct.
    unsafe { malloc_zone_statistics(std::ptr::null_mut(), &mut stats) };
    [
        stats.blocks_in_use.into(),
        stats.size_in_use as u64,
        stats.max_size_in_use as u64,
        stats.size_allocated as u64,
    ]
}

#[cfg(not(target_os = "macos"))]
fn process_allocator_stats() -> [u64; 4] {
    [0; 4]
}

#[allow(
    unsafe_code,
    reason = "delegates Rust allocations to System while counting them"
)]
unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let result = unsafe { System.alloc(layout) };
        if !result.is_null() {
            record_alloc(layout.size());
        }
        result
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let result = unsafe { System.alloc_zeroed(layout) };
        if !result.is_null() {
            ALLOC_ZEROED_CALLS.fetch_add(1, Ordering::Relaxed);
            record_alloc(layout.size());
        }
        result
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        record_dealloc(layout.size());
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let result = unsafe { System.realloc(ptr, layout, new_size) };
        if !result.is_null() {
            record_realloc(layout.size(), new_size);
        }
        result
    }
}

pub fn snapshot() -> Vec<(&'static str, u64)> {
    let alloc_calls = ALLOC_CALLS.load(Ordering::Relaxed);
    let alloc_zeroed_calls = ALLOC_ZEROED_CALLS.load(Ordering::Relaxed);
    let realloc_calls = REALLOC_CALLS.load(Ordering::Relaxed);
    let dealloc_calls = DEALLOC_CALLS.load(Ordering::Relaxed);
    let allocated_bytes = ALLOCATED_BYTES.load(Ordering::Relaxed);
    let deallocated_bytes = DEALLOCATED_BYTES.load(Ordering::Relaxed);
    let live_blocks = LIVE_BLOCKS.load(Ordering::Relaxed);
    let live_bytes = LIVE_BYTES.load(Ordering::Relaxed);
    let peak_live_bytes = PEAK_LIVE_BYTES.load(Ordering::Relaxed);
    let bucket_blocks: [u64; 9] =
        std::array::from_fn(|index| LIVE_BUCKET_BLOCKS[index].load(Ordering::Relaxed));
    let strings = bex_str::allocation_stats();
    let allocator = process_allocator_stats();

    let mut values = vec![
        ("allocator_blocks_in_use", allocator[0]),
        ("allocator_bytes_in_use", allocator[1]),
        ("allocator_bytes_high_water", allocator[2]),
        ("allocator_bytes_reserved", allocator[3]),
        ("rust_alloc_calls_total", alloc_calls),
        ("rust_alloc_zeroed_calls_total", alloc_zeroed_calls),
        ("rust_realloc_calls_total", realloc_calls),
        ("rust_dealloc_calls_total", dealloc_calls),
        ("rust_allocated_bytes_total", allocated_bytes),
        ("rust_deallocated_bytes_total", deallocated_bytes),
        ("rust_live_blocks", live_blocks),
        ("rust_live_bytes", live_bytes),
        ("rust_peak_live_bytes", peak_live_bytes),
    ];
    values.extend(BUCKET_NAMES.into_iter().zip(bucket_blocks));
    values.extend([
        (
            "bex_str_flat_nodes_created_total",
            strings.flat_nodes_created,
        ),
        (
            "bex_str_flat_nodes_dropped_total",
            strings.flat_nodes_dropped,
        ),
        ("bex_str_flat_nodes_live", strings.flat_nodes_live),
        (
            "bex_str_flat_payloads_created_total",
            strings.flat_payloads_created,
        ),
        (
            "bex_str_flat_payloads_dropped_total",
            strings.flat_payloads_dropped,
        ),
        ("bex_str_flat_payloads_live", strings.flat_payloads_live),
        (
            "bex_str_flat_payload_bytes_created_total",
            strings.flat_payload_bytes_created,
        ),
        (
            "bex_str_flat_payload_bytes_dropped_total",
            strings.flat_payload_bytes_dropped,
        ),
        (
            "bex_str_flat_payload_bytes_live",
            strings.flat_payload_bytes_live,
        ),
        (
            "bex_str_concat_nodes_created_total",
            strings.concat_nodes_created,
        ),
        (
            "bex_str_concat_nodes_dropped_total",
            strings.concat_nodes_dropped,
        ),
        ("bex_str_concat_nodes_live", strings.concat_nodes_live),
        ("bex_str_flatten_calls_total", strings.flatten_calls),
    ]);
    values
}
