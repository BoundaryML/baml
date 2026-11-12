//! BEX Unified Heap
//!
//! The unified heap for the BEX virtual machine:
//!
//! - **Block space with a copying collector**: 32 KiB blocks of 16-byte
//!   granules with object-start, mark, pin and hooked bitmaps per block,
//!   bump-filled with small cells of mixed sizes, and separately allocated
//!   large objects ([`space`]). A collection copies every unpinned live
//!   small cell into bump-filled to-space blocks; a cell a conservative root
//!   names is pinned and stays, and so does its block.
//! - **Per-mutator allocation**: each VM bumps into a block of its own
//!   without locking ([`Tlab`]).
//! - **Descriptors**: every cell starts with a [`Header`] naming the
//!   [`Descriptor`] that says where its pointers are. The VM's `Object` enum
//!   is one descriptor, traced by its existing enum-specific code.
//! - **Roots**: precise roots from handles and the root registry, plus
//!   conservative word ranges ([`RootSource`]), gathered under the
//!   [`HeapGuard`] pause.
//! - **Handle table**: stable keys for objects held across the FFI boundary.
//!
//! # Crate Dependencies
//!
//! ```text
//! bex_vm_types ◄── bex_external_types ◄── bex_heap ◄── bex_vm ◄── bex_engine
//! (internal)       (FFI boundary)         (memory)     (exec)     (async)
//! ```

#![warn(clippy::disallowed_methods)]

mod accessor;
mod descriptor;
mod functions;
mod gc;
pub mod gc_policy;
mod gc_profile;
mod heap;
mod heap_debugger;
mod heap_guard;
mod object_trace;
mod root_source;
pub mod space;
mod tlab;
mod verify;

// Re-export types from bex_external_types for convenience
pub use accessor::{AccessError, BexClass, BexValue, BuiltinClass};
pub use bex_external_types::{BexExternalValue, Handle, WeakHeapRef};
pub use bex_vm_types::PermitProof;
pub use descriptor::{
    Descriptor, DescriptorError, DescriptorId, DescriptorTable, ElemKind, FinalizerId, Header,
    Kind, OBJECT_DESC, RawKind, RustHooks, SlotKind, TaggedKind, TaggedSlot, Trace, TypeId,
};
pub use gc::{GcStats, Restamp};
#[cfg(feature = "gc_profiling")]
pub use gc_profile::{ConservativeScan, GcHeapSnapshot};
pub use gc_profile::{GcCycleProfiler, GcProfile};
pub use heap::{BexHeap, Finalizable, Generation, HeapStats, UnhandledSpawnError};
pub(crate) use heap_debugger::{HeapDebuggerConfig, HeapDebuggerState};
pub use heap_guard::{
    ActiveHeapPermit, HeapGuard, HeapPermit, HeapPermitManager, InactiveHeapPermit,
};
pub use root_source::{BothRoots, ConservativeRanges, NoRoots, PreciseRoots, RootSource};
pub use space::blocks::BlockOccupancy;
pub use tlab::{Reservation, SETTLE_QUANTUM, Tlab, TlabHolder};
