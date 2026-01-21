//! BEX Unified Heap
//!
//! This crate provides the unified heap for the BEX virtual machine,
//! implementing a CLR/JVM-style architecture with:
//!
//! - **Lock-free field writes**: `UnsafeCell<Vec<Object>>` enables direct memory access
//! - **Per-VM TLABs**: Thread-Local Allocation Buffers prevent allocation contention
//! - **Handle table**: `sharded_slab` provides lock-free handle management for FFI
//!
//! # Architecture Overview
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                         BexEngine                                │
//! │                     owns Arc<BexHeap>                           │
//! └───────────────────────┬─────────────────────────────────────────┘
//!                         │ Arc::clone() per call_function
//!         ┌───────────────┼───────────────────┐
//!         ▼               ▼                   ▼
//!    ┌─────────┐     ┌─────────┐         ┌─────────┐
//!    │  BexVm  │     │  BexVm  │         │  BexVm  │  (concurrent)
//!    │  (VM1)  │     │  (VM2)  │         │  (VM3)  │
//!    └────┬────┘     └────┬────┘         └────┬────┘
//!         │ owns          │ owns              │ owns
//!         ▼               ▼                   ▼
//!    ┌─────────┐     ┌─────────┐         ┌─────────┐
//!    │  Tlab1  │     │  Tlab2  │         │  Tlab3  │  (exclusive)
//!    └─────────┘     └─────────┘         └─────────┘
//! ```
//!
//! # Memory Layout
//!
//! The heap maintains a single contiguous `Vec<Object>` with a boundary
//! separating compile-time objects (permanent) from runtime objects (collectible):
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────────┐
//! │  [0] [1] [2] ... [N-1] ║ [N] [N+1] ... [M-1] [M] ...            │
//! │  ◄─ Compile-Time ────► ║ ◄────── Runtime (TLAB regions) ──────► │
//! │     (permanent)        ║        (collectible by GC)              │
//! │                        ▲                                         │
//! │               compile_time_boundary                              │
//! └──────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Thread Safety
//!
//! The heap uses `UnsafeCell` for lock-free access. Safety is ensured by:
//!
//! - **TLAB exclusivity**: Each VM allocates into its own reserved region
//! - **No shared mutation**: BAML has no global mutable variables
//! - **Safepoint GC**: Collection only runs when all VMs are yielded
//!
//! # Crate Dependencies
//!
//! ```text
//! bex_vm_types ◄── bex_external_types ◄── bex_heap ◄── bex_vm ◄── bex_engine
//! (internal)       (FFI boundary)         (memory)     (exec)     (async)
//! ```

mod accessor;
mod gc;
mod heap;
mod heap_debugger;
mod tlab;

// Re-export types from bex_external_types for convenience
pub use accessor::GcProtectedHeap;
pub use bex_external_types::{BexExternalValue, BexValue, Handle};
pub use gc::GcStats;
pub use heap::{BexHeap, DEFAULT_TLAB_SIZE, HeapStats};
pub(crate) use heap_debugger::HeapDebuggerState;
pub use heap_debugger::{HeapDebuggerConfig, HeapVerifyMode};
pub use tlab::{Tlab, TlabChunk};
