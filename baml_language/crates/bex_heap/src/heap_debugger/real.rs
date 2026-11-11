use std::sync::atomic::{AtomicU32, Ordering};

use bex_vm_types::{FutureRead, HeapPtr, Object, Value};

use crate::BexHeap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeapVerifyMode {
    Off,
    Quick,
    Full,
}

impl HeapVerifyMode {
    const CHOICES: [(&'static str, Self); 3] = [
        ("off", Self::Off),
        ("quick", Self::Quick),
        ("full", Self::Full),
    ];
}

#[derive(Clone, Copy, Debug)]
pub struct HeapDebuggerConfig {
    pub enabled: bool,
    pub verify: HeapVerifyMode,
}

impl Default for HeapDebuggerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            verify: HeapVerifyMode::Off,
        }
    }
}

impl HeapDebuggerConfig {
    /// Reads `DEV_BAML_HEAP_VERIFY=off|quick|full`; any value other than
    /// `off` enables the debugger. This is a dev-only, feature-gated knob, so
    /// an invalid value panics with a clear message instead of being ignored.
    pub fn from_env() -> Self {
        let verify = baml_env::choice_var("DEV_BAML_HEAP_VERIFY", &HeapVerifyMode::CHOICES)
            .unwrap_or_else(|error| panic!("{error}"))
            .unwrap_or(HeapVerifyMode::Off);
        Self {
            enabled: verify != HeapVerifyMode::Off,
            verify,
        }
    }
}

pub(crate) struct HeapDebuggerState {
    config: HeapDebuggerConfig,
    epoch: AtomicU32,
}

impl HeapDebuggerState {
    pub(crate) fn new(config: HeapDebuggerConfig) -> Self {
        Self {
            config,
            epoch: AtomicU32::new(0),
        }
    }

    pub(crate) fn config(&self) -> &HeapDebuggerConfig {
        &self.config
    }

    pub(crate) fn bump_epoch(&self) -> u32 {
        self.epoch.fetch_add(1, Ordering::AcqRel) + 1
    }

    pub(crate) fn epoch(&self) -> u32 {
        self.epoch.load(Ordering::Acquire)
    }
}

impl BexHeap {
    pub fn debug_config(&self) -> &HeapDebuggerConfig {
        self.debug_state().config()
    }

    pub fn verify_quick(&self) {
        let debug = self.debug_state().config();
        if !debug.enabled {
            return;
        }

        match debug.verify {
            HeapVerifyMode::Quick => {
                self.verify_quick_impl();
            }
            HeapVerifyMode::Full => {
                self.verify_quick_impl();
                self.verify_full_impl();
            }
            HeapVerifyMode::Off => {}
        }
    }

    fn verify_quick_impl(&self) {
        let handles = self.handles.read().expect("handles lock poisoned");
        for (handle_key, idx) in handles.iter() {
            let ptr_addr = idx.as_ptr() as usize;
            assert!(
                ptr_addr != 0,
                "handle has null pointer: handle_key={handle_key}"
            );
            // Note: With HeapPtr, we can't easily do bounds checking since we have raw pointers
            // The epoch check in debug_assert_valid_index provides safety guarantees
            let _ = ptr_addr; // Silence unused warning - we verified it's not null
        }
    }

    fn verify_full_impl(&self) {
        let ct_len = self.compile_time_len();
        // Verify compile-time objects
        for raw in 0..ct_len {
            let idx = self.compile_time_ptr(raw);
            let obj = unsafe { idx.get() };
            self.verify_object_invariants(idx, obj, ct_len);
        }

        // Verify every runtime object. The objects are gathered first: the
        // checks look pointers up in the space, which takes its locks.
        let mut objects = Vec::new();
        {
            let blocks = self.blocks.lock();
            let large = self.large.lock();
            // SAFETY: verification runs with every mutator parked.
            unsafe {
                Self::for_each_runtime_object(blocks.pool(), &large, |object| objects.push(object));
            }
        }
        self.verify_slot_cells();
        for object in objects {
            // SAFETY: allocated, and every mutator is parked.
            unsafe {
                let idx = HeapPtr::from_ptr(object, self.heap_epoch());
                self.verify_object_invariants(idx, &*object, ct_len);
            }
        }

        let handles = self.handles.read().expect("handles lock poisoned");
        for (handle_key, idx) in handles.iter() {
            self.debug_assert_valid_index(*idx);
            let obj = unsafe { self.get_object(*idx) };
            if let Object::Sentinel(_) | Object::Tombstone = obj {
                panic!("handle points to sentinel: handle_key={handle_key} idx={idx:?}");
            }
        }
    }

    /// Every pointer a shared-format cell (instance, list, backing, map)
    /// holds names an allocated cell or a compile-time object.
    fn verify_slot_cells(&self) {
        let mut cells = Vec::new();
        {
            let blocks = self.blocks.lock();
            let large = self.large.lock();
            // SAFETY: verification runs with every mutator parked.
            unsafe {
                Self::for_each_runtime_cell(blocks.pool(), &large, |cell| cells.push(cell));
            }
        }
        let mut edges = Vec::new();
        for cell in cells {
            // SAFETY: an allocated cell with a written header.
            let id =
                unsafe { <crate::Header as crate::descriptor::CellHeader>::at(cell) }.descriptor;
            let descriptor = self.descriptors().get(id);
            if matches!(descriptor.trace, crate::Trace::Rust(_)) {
                continue;
            }
            // SAFETY: allocated and laid out by its descriptor; mutators are
            // parked.
            unsafe {
                crate::descriptor::trace_slots(
                    descriptor,
                    cell,
                    crate::space::cell_bytes(cell),
                    &mut edges,
                );
            }
            for edge in edges.drain(..) {
                self.debug_assert_valid_index(edge);
            }
        }
    }

    fn verify_object_invariants(&self, idx: HeapPtr, obj: &Object, _ct_len: usize) {
        match obj {
            // Shared-format objects are not enum cells; `verify_slot_cells`
            // checks their pointer slots.
            Object::Shared(_) => {}
            Object::Variant(variant) => {
                let enm_idx = variant.enm;
                self.debug_assert_valid_index(enm_idx);
                let enm_obj = unsafe { self.get_object(enm_idx) };
                let Object::Enum(enm) = enm_obj else {
                    panic!("variant.enm not Enum: obj_idx={idx:?} enm_idx={enm_idx:?}");
                };
                assert!(
                    variant.index < enm.variants.len(),
                    "variant index out of bounds: obj_idx={idx:?} variant_index={} enum_len={}",
                    variant.index,
                    enm.variants.len()
                );
            }
            Object::Future(fut) => match fut.read() {
                FutureRead::Ready(value) | FutureRead::Error(value) => {
                    self.debug_assert_valid_value(&value);
                }
                FutureRead::Pending(_)
                | FutureRead::Cancelled
                | FutureRead::InternalError(_) => {}
            },
            Object::Closure(closure) => {
                self.debug_assert_valid_index(closure.function);
                for value in &closure.captures {
                    self.debug_assert_valid_value(value);
                }
            }
            Object::BoundMethod(bm) => {
                self.debug_assert_valid_index(bm.function);
                self.debug_assert_valid_value(&bm.receiver);
            }
            Object::Cell(cell) => {
                self.debug_assert_valid_value(&cell.load());
            }
            Object::Function(_)
            | Object::GenericFunction(_)
            | Object::Class(_)
            | Object::Enum(_)
            // Compile-time program metadata; their pointers target other
            // immortal compile-time objects, valid by construction.
            | Object::Interface(_)
            | Object::Package(_)
            | Object::ImplRule(_)
            | Object::TypeAlias(_)
            | Object::String(_)
            | Object::Bigint(_)
            | Object::Uint8Array(_)
            | Object::RustData(_)

            | Object::Type(_)
            | Object::Float(_)
            // `HostClosure` carries no heap references.
            | Object::HostClosure(_)
            | Object::Tombstone => {}
            #[cfg(feature = "heap_debug")]
            Object::Sentinel(_) => {}
        }
    }

    fn debug_assert_valid_value(&self, value: &Value) {
        if let Some(idx) = value.as_object_ptr() {
            let _ = unsafe { self.get_object(idx) };
        }
    }

    pub fn debug_assert_valid_index(&self, idx: HeapPtr) {
        let debug = self.debug_state().config();
        if !debug.enabled {
            return;
        }

        // Check the pointer is not null
        assert!(!idx.as_ptr().is_null(), "heap pointer is null");
        assert!(
            self.debug_ptr_is_allocated(idx),
            "heap pointer names no allocated object: ptr={:?}",
            idx.as_ptr()
        );

        // No epoch equality is asserted. Epochs still stamp every pointer the
        // heap mints, and each collection re-stamps the roots and handles it
        // saw (its `Restamp` map), so a pointer's epoch says which collection
        // last vouched for it. But a pointer stored in an object is rewritten
        // by the collector with a zero epoch, and one to a pinned object is
        // never rewritten at all, so stored pointers legitimately outlive
        // their epoch: liveness is checked above, against the space, instead.
    }

    /// Whether `ptr` names a compile-time object or an allocated cell. Reads
    /// only the space's tables, never the object.
    pub fn debug_ptr_is_allocated(&self, ptr: HeapPtr) -> bool {
        use crate::space::{block::BlockHeader, cell_of};
        if self.is_compile_time_ptr(ptr) {
            return true;
        }
        let cell = cell_of(ptr.as_ptr() as usize);
        if let Some(base) = self.blocks.lock().pool().block_containing(cell) {
            // SAFETY: an initialized block; only its tables are read.
            let header = unsafe { BlockHeader::at(base) };
            return header.starts.get(BlockHeader::granule(base, cell));
        }
        self.large.lock().contains_cell(cell)
    }

    pub(crate) fn bump_epoch(&self) -> u32 {
        self.debug_state().bump_epoch()
    }

    /// Whether `ptr`'s object was reclaimed: a root still naming one after
    /// the roots were forwarded was missed by the root scan.
    pub fn debug_ptr_is_reclaimed(&self, ptr: HeapPtr) -> bool {
        !self.debug_ptr_is_allocated(ptr)
    }

    #[allow(dead_code)]
    pub(crate) fn heap_epoch(&self) -> u32 {
        self.debug_state().epoch()
    }

    pub(crate) fn debug_assert_not_sentinel(&self, obj: &Object) {
        let debug = self.debug_state().config();
        if !debug.enabled {
            return;
        }

        match obj {
            Object::Sentinel(kind) => panic!("heap sentinel read: {kind:?}"),
            Object::Tombstone => panic!("heap tombstone read"),
            _ => {}
        }
    }

    /// Create a HeapPtr from a raw pointer.
    /// In debug mode, includes the current epoch for stale pointer detection.
    #[inline]
    pub(crate) unsafe fn make_heap_ptr(&self, ptr: *mut Object) -> HeapPtr {
        unsafe { HeapPtr::from_ptr(ptr, self.heap_epoch()) }
    }
}
