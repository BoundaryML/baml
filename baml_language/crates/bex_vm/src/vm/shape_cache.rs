#![allow(unsafe_code)]

use std::cell::Cell;

use bex_vm_types::{ClassShape, FieldRef, HeapPtr, InstanceRef};

const ENTRIES: usize = 64;

pub(crate) struct ShapeCache {
    entries: [Cell<(u64, ClassShape)>; ENTRIES],
}

impl ShapeCache {
    pub(crate) fn new() -> Self {
        Self {
            entries: std::array::from_fn(|_| Cell::new((0, ClassShape::EMPTY))),
        }
    }

    #[allow(clippy::inline_always, clippy::cast_possible_truncation)]
    #[inline(always)]
    fn index(class: u64) -> usize {
        ((class >> 4) ^ (class >> 10)) as usize % ENTRIES
    }

    /// # Safety
    ///
    /// `ptr` names a live cell.
    #[allow(clippy::inline_always)]
    #[inline(always)]
    pub(crate) unsafe fn field(&self, ptr: HeapPtr, field: usize) -> Option<FieldRef> {
        let (instance, class) = unsafe { InstanceRef::with_class_bits(ptr) }?;
        let (key, shape) = self.entries[Self::index(class)].get();
        if key == class {
            return unsafe { instance.field_in(shape, field) };
        }
        self.miss(instance, class, field)
    }

    #[cold]
    #[inline(never)]
    fn miss(&self, instance: InstanceRef, class: u64, field: usize) -> Option<FieldRef> {
        let Some(shape) = ClassShape::of(instance.layout()) else {
            return instance.field(field);
        };
        self.entries[Self::index(class)].set((class, shape));
        unsafe { instance.field_in(shape, field) }
    }

    pub(crate) fn clear(&self) {
        for entry in &self.entries {
            entry.set((0, ClassShape::EMPTY));
        }
    }
}
