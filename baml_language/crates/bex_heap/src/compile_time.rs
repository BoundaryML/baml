//! The compile-time space: the program image's objects, laid out once in
//! one arena at stable addresses and never collected.
//!
//! Each pool object becomes one cell. A string literal becomes a flat string
//! cell (`STR_DESC`), the same cell `Tlab::alloc_string` makes at run time,
//! so a literal and a runtime string read alike. Every other object is an
//! `Object` enum cell (`OBJECT_DESC`). Cells are granule-aligned, so the
//! arena is one contiguous range the collector tests pointers against.

use std::{alloc::Layout, ops::Range, ptr::NonNull};

use bex_vm_types::{
    HeapPtr, Object,
    object_format::{HEADER_BYTES, Header},
    types::{STR_DESC, StrRef, str_cell_bytes},
};

use crate::{
    descriptor::{OBJECT_CELL_BYTES, OBJECT_DESC, RUST_PAYLOAD_OFFSET},
    space::{GRANULE_BYTES, granule_align},
};

const _: () = assert!(bex_vm_types::types::STR_GRANULE_BYTES == GRANULE_BYTES);

pub(crate) struct CompileTimeSpace {
    /// The arena, `bytes` long; dangling when `bytes` is zero.
    arena: NonNull<u8>,
    bytes: usize,
    /// Byte offset, from the arena, of each pool index's cell (its header).
    cells: Vec<usize>,
}

// SAFETY: the arena is immutable after sealing but for a string cell's hash
// word, which is atomic; the objects in it are `Send + Sync`.
unsafe impl Send for CompileTimeSpace {}
unsafe impl Sync for CompileTimeSpace {}

impl CompileTimeSpace {
    /// Lay `objects` out, in pool order.
    pub(crate) fn new(objects: Vec<Object>) -> Self {
        let sizes: Vec<usize> = objects.iter().map(Self::cell_bytes).collect();
        let bytes: usize = sizes.iter().sum();
        let cells: Vec<usize> = sizes
            .iter()
            .scan(0, |next, &size| {
                let offset = *next;
                *next += size;
                Some(offset)
            })
            .collect();
        let arena = if bytes == 0 {
            NonNull::dangling()
        } else {
            // SAFETY: a non-zero, granule-aligned size.
            let raw = unsafe { std::alloc::alloc(Self::layout(bytes)) };
            NonNull::new(raw).unwrap_or_else(|| std::alloc::handle_alloc_error(Self::layout(bytes)))
        };
        let space = Self {
            arena,
            bytes,
            cells,
        };
        for (index, object) in objects.into_iter().enumerate() {
            // SAFETY: the cell at `index` was sized for `object`.
            unsafe { space.write(index, object) };
        }
        space
    }

    fn layout(bytes: usize) -> Layout {
        Layout::from_size_align(bytes, GRANULE_BYTES).expect("a compile-time arena fits a layout")
    }

    fn cell_bytes(object: &Object) -> usize {
        match object {
            Object::String(text) => str_cell_bytes(text.byte_len()),
            _ => granule_align(OBJECT_CELL_BYTES),
        }
    }

    /// Write `object` into its cell: a string literal as a flat string cell,
    /// anything else as an enum cell.
    ///
    /// # Safety
    /// The cell at `index` is sized for `object` and holds nothing yet.
    unsafe fn write(&self, index: usize, object: Object) {
        let cell = self.cell(index);
        // SAFETY: inside the arena, at a granule.
        unsafe {
            if let Object::String(text) = &object {
                cell.cast::<Header>().write(Header::new(STR_DESC, 0));
                StrRef::init(self.heap_ptr(index), text.as_str());
            } else {
                cell.cast::<Header>().write(Header::new(OBJECT_DESC, 0));
                cell.add(RUST_PAYLOAD_OFFSET).cast::<Object>().write(object);
            }
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.cells.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Addresses of the arena.
    pub(crate) fn range(&self) -> Range<usize> {
        let start = self.arena.as_ptr() as usize;
        start..start + self.bytes
    }

    /// The cell (its header) of pool index `index`.
    #[inline]
    fn cell(&self, index: usize) -> *mut u8 {
        assert!(
            index < self.cells.len(),
            "compile-time index {index} out of bounds (len={})",
            self.cells.len()
        );
        // SAFETY: inside the arena.
        unsafe { self.arena.as_ptr().add(self.cells[index]) }
    }

    /// The payload pointer of pool index `index`, as a `HeapPtr` with no
    /// epoch.
    #[inline]
    pub(crate) fn heap_ptr(&self, index: usize) -> HeapPtr {
        crate::object_trace::heap_ptr_at(self.cell(index) as usize + HEADER_BYTES)
    }

    #[inline]
    fn is_object(&self, index: usize) -> bool {
        // SAFETY: every cell has its header written.
        unsafe { &*self.cell(index).cast::<Header>() }.descriptor == OBJECT_DESC
    }

    /// The enum at pool index `index`; `None` for a string literal, which is
    /// a flat string cell.
    pub(crate) fn object(&self, index: usize) -> Option<&Object> {
        self.is_object(index)
            // SAFETY: an enum cell's payload is a live `Object`.
            .then(|| unsafe { &*self.cell(index).add(RUST_PAYLOAD_OFFSET).cast::<Object>() })
    }

    /// [`Self::object`], mutably.
    pub(crate) fn object_mut(&mut self, index: usize) -> Option<&mut Object> {
        self.is_object(index)
            // SAFETY: an enum cell's payload is a live `Object`; `&mut self`
            // makes the borrow exclusive.
            .then(|| unsafe { &mut *self.cell(index).add(RUST_PAYLOAD_OFFSET).cast::<Object>() })
    }

    /// Every enum object, in pool order.
    pub(crate) fn objects(&self) -> impl Iterator<Item = &Object> {
        (0..self.len()).filter_map(|index| self.object(index))
    }

    /// Every enum object with its index, mutably, in pool order.
    pub(crate) fn for_each_object_mut(&mut self, mut f: impl FnMut(usize, &mut Object)) {
        for index in 0..self.len() {
            if let Some(object) = self.object_mut(index) {
                f(index, object);
            }
        }
    }

    /// Replace the enum at `index`, dropping what was there.
    ///
    /// # Panics
    /// If `index` is a string literal: a literal is never a placeholder.
    pub(crate) fn set_object(&mut self, index: usize, object: Object) {
        assert!(
            !matches!(object, Object::String(_)),
            "a compile-time string is laid out when the heap is built"
        );
        let slot = self
            .object_mut(index)
            .unwrap_or_else(|| panic!("compile-time index {index} is a string literal"));
        *slot = object;
    }
}

impl Drop for CompileTimeSpace {
    fn drop(&mut self) {
        for index in 0..self.len() {
            if let Some(object) = self.object_mut(index) {
                // SAFETY: each enum cell is dropped once, here.
                unsafe { std::ptr::drop_in_place(object) };
            }
        }
        if self.bytes != 0 {
            // SAFETY: allocated in `new` with this layout.
            unsafe { std::alloc::dealloc(self.arena.as_ptr(), Self::layout(self.bytes)) };
        }
    }
}
