use std::sync::atomic::{AtomicU32, Ordering};

use bex_vm_types::{FutureRead, HeapPtr, InstanceRef, Object, Value};

use crate::{BexHeap, cell_space::CellBody};

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
        // SAFETY: verification runs with every allocator parked.
        let gen0 = unsafe { &*self.gen0.get() };
        gen0.for_each_cell(|cell| {
            // SAFETY: as above; outside a collection no cell is forwarded.
            self.verify_cell(unsafe { CellBody::at(cell) });
            // SAFETY: as above.
            unsafe { self.cell_words(cell) }
        });

        let handles = self.handles.read().expect("handles lock poisoned");
        for (handle_key, ptr) in handles.iter() {
            assert!(
                !ptr.as_ptr().is_null(),
                "handle has null pointer: handle_key={handle_key}"
            );
        }
    }

    fn verify_full_impl(&self) {
        for raw in 0..self.compile_time_len() {
            let ptr = self.compile_time_ptr(raw);
            // SAFETY: compile-time objects live as long as the heap.
            self.verify_object_invariants(ptr, unsafe { ptr.get() });
        }

        let handles = self.handles.read().expect("handles lock poisoned");
        for (handle_key, ptr) in handles.iter() {
            self.debug_assert_valid_index(*ptr);
            // SAFETY: a handle keeps its object alive.
            let obj = unsafe { self.get_object(*ptr) };
            if let Object::Sentinel(_) | Object::Tombstone = obj {
                panic!("handle points to sentinel: handle_key={handle_key} ptr={ptr:?}");
            }
        }
    }

    fn verify_cell(&self, body: CellBody) {
        match body {
            CellBody::Instance(instance) => self.verify_instance(instance),
            // SAFETY: the caller walks intact cells.
            CellBody::Object(object) => {
                // SAFETY: as above.
                let obj = unsafe { &*object };
                if let Object::Tombstone = obj {
                    return;
                }
                // SAFETY: the object is in a live cell.
                let ptr = unsafe { self.make_heap_ptr(object) };
                self.verify_object_invariants(ptr, obj);
            }
        }
    }

    fn verify_instance(&self, instance: InstanceRef) {
        for slot in instance.layout().value_slots() {
            // SAFETY: the layout names these value slots.
            self.debug_assert_valid_value(&unsafe { instance.slot_value(slot) });
        }
    }

    fn verify_object_invariants(&self, idx: HeapPtr, obj: &Object) {
        match obj {
            // SAFETY: heap-debugger verification runs under STW (it's
            // called from the GC verifier path); no mutator is concurrently
            // active.
            Object::Array(values) => {
                let data = unsafe { values.data_unchecked() };
                for value in data.iter() {
                    self.debug_assert_valid_value(value);
                }
            }
            Object::Map(values) => {
                let data = unsafe { values.data_unchecked() };
                for value in data.keys().chain(data.values()) {
                    self.debug_assert_valid_value(value);
                }
            }
            Object::Tuple(tuple) => {
                assert!(
                    tuple.element_tys.len() == tuple.len(),
                    "tuple shape mismatch: obj_idx={idx:?} types_len={} data_len={}",
                    tuple.element_tys.len(),
                    tuple.len(),
                );
                for value in tuple.slot_values() {
                    self.debug_assert_valid_value(&value);
                }
            }
            Object::Instance => panic!("an object slot holds an instance: obj_idx={idx:?}"),
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

        // Check epoch matches
        let current_epoch = self.heap_epoch();
        let idx_epoch = idx.epoch();
        assert!(
            idx_epoch == current_epoch,
            "heap pointer epoch mismatch: idx_epoch={idx_epoch} heap_epoch={current_epoch} ptr={:?}",
            idx.as_ptr()
        );
    }

    pub(crate) fn bump_epoch(&self) -> u32 {
        self.debug_state().bump_epoch()
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
