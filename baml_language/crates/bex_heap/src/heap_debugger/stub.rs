use bex_vm_types::{Object, ObjectIndex};

use crate::BexHeap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeapVerifyMode {
    Off,
    Quick,
    Full,
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
        Self::default()
    }
}

pub(crate) struct HeapDebuggerState {
    config: HeapDebuggerConfig,
}

impl HeapDebuggerState {
    pub(crate) fn new(config: HeapDebuggerConfig) -> Self {
        Self { config }
    }

    pub(crate) fn config(&self) -> &HeapDebuggerConfig {
        &self.config
    }

    pub(crate) fn bump_epoch(&self) -> u32 {
        0
    }

    pub(crate) fn epoch(&self) -> u32 {
        0
    }
}

impl<F> BexHeap<F> {
    #[inline]
    pub fn debug_config(&self) -> &HeapDebuggerConfig {
        self.debug_state().config()
    }

    #[inline]
    pub fn verify_quick(&self) {}

    #[inline]
    pub fn debug_assert_valid_index(&self, _idx: ObjectIndex) {}

    #[inline]
    pub(crate) fn record_tlab_canary(&self, _idx: usize) {}

    #[inline]
    pub(crate) fn clear_tlab_canaries(&self) {}

    #[inline]
    pub(crate) fn debug_verify_tlab_canaries(&self) {}

    #[inline]
    pub(crate) fn bump_epoch(&self) -> u32 {
        self.debug_state().bump_epoch()
    }

    #[allow(dead_code)]
    pub(crate) fn heap_epoch(&self) -> u32 {
        self.debug_state().epoch()
    }

    pub(crate) fn make_object_index(&self, raw: usize) -> ObjectIndex {
        ObjectIndex::from_raw(raw)
    }

    pub(crate) fn placeholder_object(&self) -> Object<F> {
        Object::String(String::new())
    }

    pub(crate) fn tlab_canary_object(&self, _chunk_start: usize, _chunk_end: usize) -> Object<F> {
        Object::String(String::new())
    }

    pub(crate) fn finalize_from_space(&self, from_space: usize) {
        unsafe {
            (*self.spaces[from_space].get()).clear();
        }
    }

    #[inline]
    pub(crate) fn debug_assert_not_sentinel(&self, _obj: &Object<F>) {}
}
