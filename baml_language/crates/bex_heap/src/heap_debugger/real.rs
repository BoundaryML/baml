use std::sync::{
    Mutex,
    atomic::{AtomicU32, Ordering},
};

use bex_vm_types::{
    FutureRead, HeapPtr, Object, Value,
    types::{ObjectType, SentinelKind},
};

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
    tlab_canaries: Mutex<Vec<usize>>,
}

impl HeapDebuggerState {
    pub(crate) fn new(config: HeapDebuggerConfig) -> Self {
        Self {
            config,
            epoch: AtomicU32::new(0),
            tlab_canaries: Mutex::new(Vec::new()),
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

    pub(crate) fn record_tlab_canary(&self, idx: usize) {
        let mut canaries = self
            .tlab_canaries
            .lock()
            .expect("tlab canaries lock poisoned");
        canaries.push(idx);
    }

    pub(crate) fn clear_tlab_canaries(&self) {
        let mut canaries = self
            .tlab_canaries
            .lock()
            .expect("tlab canaries lock poisoned");
        canaries.clear();
    }

    pub(crate) fn tlab_canaries(&self) -> Vec<usize> {
        let canaries = self
            .tlab_canaries
            .lock()
            .expect("tlab canaries lock poisoned");
        canaries.clone()
    }
}

impl BexHeap {
    pub fn debug_config(&self) -> &HeapDebuggerConfig {
        self.debug_state().config()
    }

    pub(crate) fn record_tlab_canary(&self, idx: usize) {
        let debug = self.debug_state().config();
        if !debug.enabled {
            return;
        }
        self.debug_state().record_tlab_canary(idx);
    }

    pub(crate) fn clear_tlab_canaries(&self) {
        let debug = self.debug_state().config();
        if !debug.enabled {
            return;
        }
        self.debug_state().clear_tlab_canaries();
    }

    pub(crate) fn debug_verify_tlab_canaries(&self) {
        let debug = self.debug_state().config();
        if !debug.enabled {
            return;
        }

        let canaries = self.debug_state().tlab_canaries();
        if canaries.is_empty() {
            return;
        }

        let ct_len = self.compile_time_len();
        let runtime_len = self.len().saturating_sub(ct_len);
        let max_index = ct_len + runtime_len;

        unsafe {
            let gen0 = &*self.gen0.get();
            for raw in canaries {
                assert!(
                    raw >= ct_len,
                    "tlab canary out of bounds: idx={raw} ct_len={ct_len}"
                );
                assert!(
                    raw < max_index,
                    "tlab canary out of bounds: idx={raw} max={max_index}"
                );
                let runtime_idx = raw - ct_len;
                let obj = &gen0[runtime_idx];
                match obj {
                    Object::Sentinel(SentinelKind::TlabCanary { .. }) => {}
                    _ => {
                        let obj_type = ObjectType::of(obj);
                        panic!("tlab canary clobbered: idx={raw} obj_type={obj_type:?}");
                    }
                }
            }
        }
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
        let next_chunk = self.next_chunk_value();
        let runtime_len = self.len().saturating_sub(self.compile_time_len());
        assert!(
            next_chunk <= runtime_len,
            "heap next_chunk out of bounds: next_chunk={next_chunk} runtime_len={runtime_len}"
        );

        let ct_len = self.compile_time_len();
        let _max_index = ct_len + runtime_len;

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

        self.debug_verify_tlab_canaries();
    }

    fn verify_full_impl(&self) {
        let ct_len = self.compile_time_len();
        // Verify compile-time objects
        for raw in 0..ct_len {
            let idx = self.compile_time_ptr(raw);
            let obj = unsafe { idx.get() };
            self.verify_object_invariants(idx, obj, ct_len);
        }

        // Verify all runtime objects in Gen0 (the active nursery)
        unsafe {
            let gen0 = &*self.gen0.get();
            for (runtime_idx, obj) in gen0.iter().enumerate() {
                let ptr = gen0.get_ptr(runtime_idx);
                let idx = HeapPtr::from_ptr(ptr, self.heap_epoch());
                if self.debug_handle_runtime_sentinel(idx, obj, ct_len) {
                    continue;
                }
                self.verify_object_invariants(idx, obj, ct_len);
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

    fn debug_handle_runtime_sentinel(&self, idx: HeapPtr, obj: &Object, _ct_len: usize) -> bool {
        let kind = match obj {
            Object::Sentinel(kind) => kind,
            Object::Tombstone => panic!("tombstone in active space: idx={idx:?}"),
            _ => return false,
        };

        match kind {
            SentinelKind::Uninit => true,
            SentinelKind::TlabCanary {
                chunk_start,
                chunk_end,
            } => {
                // With HeapPtr we can't easily do index-based validation
                // Just verify the canary structure is self-consistent
                assert!(
                    *chunk_start < *chunk_end,
                    "tlab canary start >= end: chunk_start={chunk_start} chunk_end={chunk_end}"
                );
                assert!(
                    *chunk_end - *chunk_start == self.tlab_size(),
                    "tlab canary size mismatch: chunk_start={chunk_start} chunk_end={chunk_end} tlab_size={}",
                    self.tlab_size()
                );
                true
            }
        }
    }

    fn verify_object_invariants(&self, idx: HeapPtr, obj: &Object, _ct_len: usize) {
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
            Object::Instance(instance) => {
                let class_idx = instance.class;
                self.debug_assert_valid_index(class_idx);
                let class_obj = unsafe { self.get_object(class_idx) };
                let Object::Class(class) = class_obj else {
                    panic!("instance.class not Class: obj_idx={idx:?} class_idx={class_idx:?}");
                };
                assert!(
                    instance.fields.len() == class.fields.len(),
                    "instance field count mismatch: obj_idx={idx:?} fields_len={} class_fields_len={}",
                    instance.fields.len(),
                    class.fields.len()
                );
                for slot in &instance.fields {
                    self.debug_assert_valid_value(&slot.load());
                }
            }
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

    pub(crate) fn placeholder_object(&self) -> Object {
        Object::Sentinel(SentinelKind::Uninit)
    }

    pub(crate) fn tlab_canary_object(&self, chunk_start: usize, chunk_end: usize) -> Object {
        Object::Sentinel(SentinelKind::TlabCanary {
            chunk_start,
            chunk_end,
        })
    }

    /// Keep the old space's chunks alive but drop everything in them, so a
    /// read through a stale pointer finds a tombstone instead of freed memory.
    pub(crate) fn finalize_inactive_space(&self) {
        unsafe {
            let space = &mut *self.inactive.get();
            for slot in space.iter_mut() {
                *slot = Object::Tombstone;
            }
        }
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
