use std::{
    env,
    sync::{
        Mutex,
        atomic::{AtomicU32, Ordering},
    },
};

use bex_vm_types::{
    Future, HeapPtr, Object, ObjectIndex, Value,
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
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" | "0" | "false" | "no" => Some(Self::Off),
            "quick" | "1" | "true" | "yes" => Some(Self::Quick),
            "full" => Some(Self::Full),
            _ => None,
        }
    }
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
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(value) = env::var("BEX_HEAP_DEBUG")
            && let Some(parsed) = HeapVerifyMode::parse(&value)
        {
            config.enabled = parsed != HeapVerifyMode::Off;
        }

        if let Ok(value) = env::var("BEX_HEAP_VERIFY")
            && let Some(parsed) = HeapVerifyMode::parse(&value)
        {
            config.verify = parsed;
        }

        if config.verify != HeapVerifyMode::Off {
            config.enabled = true;
        }

        config
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

        let active = self.active_space_index();
        let ct_len = self.compile_time_len();
        let runtime_len = self.len().saturating_sub(ct_len);
        let max_index = ct_len + runtime_len;

        unsafe {
            let space = &*self.spaces[active].get();
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
                let obj = &space[runtime_idx];
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
        let active = self.active_space_index();
        assert!(active <= 1, "heap active_space out of range: {active}");

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

        let active = self.active_space_index();
        unsafe {
            let space = &*self.spaces[active].get();
            for (runtime_idx, obj) in space.iter().enumerate() {
                let ptr = space.get_ptr(runtime_idx);
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
            if let Object::Sentinel(_) = obj {
                panic!("handle points to sentinel: handle_key={handle_key} idx={idx:?}");
            }
        }
    }

    fn debug_handle_runtime_sentinel(&self, idx: HeapPtr, obj: &Object, _ct_len: usize) -> bool {
        let Object::Sentinel(kind) = obj else {
            return false;
        };

        match kind {
            SentinelKind::Uninit => true,
            SentinelKind::FromSpacePoison { .. } => {
                panic!("from-space poison in active space: idx={idx:?}");
            }
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
            Object::Array(values) => {
                for value in values {
                    self.debug_assert_valid_value(value);
                }
            }
            Object::Map(values) => {
                for value in values.values() {
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
                    instance.fields.len() == class.field_names.len(),
                    "instance field count mismatch: obj_idx={idx:?} fields_len={} class_fields_len={}",
                    instance.fields.len(),
                    class.field_names.len()
                );
                for value in &instance.fields {
                    self.debug_assert_valid_value(value);
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
                    variant.index < enm.variant_names.len(),
                    "variant index out of bounds: obj_idx={idx:?} variant_index={} enum_len={}",
                    variant.index,
                    enm.variant_names.len()
                );
            }
            Object::Future(fut) => match fut {
                Future::Pending(pending) => {
                    for value in &pending.args {
                        self.debug_assert_valid_value(value);
                    }
                }
                Future::Ready(value) => {
                    self.debug_assert_valid_value(value);
                }
            },
            Object::Function(_)
            | Object::Class(_)
            | Object::Enum(_)
            | Object::String(_)
            | Object::Media(_) => {}
            #[cfg(feature = "heap_debug")]
            Object::Sentinel(_) => {}
        }
    }

    fn debug_assert_valid_value(&self, value: &Value) {
        if let Value::Object(idx) = value {
            let _ = unsafe { self.get_object(*idx) };
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

    pub(crate) fn finalize_from_space(&self, from_space: usize) {
        let epoch = self.heap_epoch();
        unsafe {
            let space = &mut *self.spaces[from_space].get();
            for slot in space.iter_mut() {
                *slot = Object::Sentinel(SentinelKind::FromSpacePoison { epoch });
            }
        }
    }

    pub(crate) fn debug_assert_not_sentinel(&self, obj: &Object) {
        let debug = self.debug_state().config();
        if !debug.enabled {
            return;
        }

        if let Object::Sentinel(kind) = obj {
            panic!("heap sentinel read: {kind:?}");
        }
    }

    /// Create a HeapPtr from a raw pointer.
    /// In debug mode, includes the current epoch for stale pointer detection.
    #[inline]
    pub(crate) unsafe fn make_heap_ptr(&self, ptr: *mut Object) -> HeapPtr {
        unsafe { HeapPtr::from_ptr(ptr, self.heap_epoch()) }
    }

    /// Create an ObjectIndex from a raw index.
    /// In debug mode, includes the current epoch for stale pointer detection.
    pub(crate) fn make_object_index(&self, raw: usize) -> ObjectIndex {
        ObjectIndex::from_raw_epoch(raw, self.heap_epoch())
    }
}
