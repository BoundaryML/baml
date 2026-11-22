//! Where a tree is built: nowhere on the first pass, which only adds up
//! the bytes, and in reserved room on the second.

use crate::{
    heap::store,
    layout::{
        ARRAY_DATA, ARRAY_LENGTH, ARRAY_SIZE, BOX_SIZE, BOX_VALUE, HEADER_FLOATS, STORAGE_CAPACITY,
        STORAGE_ELEMENTS, STRING_CHAR_COUNT_SHIFT, STRING_LENGTHS, STRING_TEXT, TYPE_ARRAY_OF_REFS,
        TYPE_ARRAY_OF_VALUES, TYPE_REFS, TYPE_STRING, TYPE_VALUES, WORD,
    },
    map,
    native::{array_size, str_size},
};

/// What the elements of an array being built are, or the values of a map.
#[derive(Clone, Copy)]
pub(super) enum Elements {
    Refs,
    Floats,
    Values,
}

pub(super) struct Out {
    /// The start of the reserved room, zero while only counting.
    base: usize,
    /// The bytes taken so far.
    used: usize,
    /// The bytes reserved.
    reserved: usize,
}

impl Out {
    pub(super) fn counting() -> Self {
        Self {
            base: 0,
            used: 0,
            reserved: 0,
        }
    }

    /// Build in the `reserved` bytes at `base`: what a counting pass over
    /// the same value added up.
    pub(super) fn building(base: usize, reserved: usize) -> Self {
        Self {
            base,
            used: 0,
            reserved,
        }
    }

    pub(super) fn bytes(&self) -> usize {
        self.used
    }

    pub(super) fn is_building(&self) -> bool {
        self.base != 0
    }

    /// Go back to having taken `bytes` bytes: what was tried since did not
    /// fit, and only a counting pass tries.
    pub(super) fn rewind(&mut self, bytes: usize) {
        debug_assert!(!self.is_building());
        self.used = bytes;
    }

    /// `bytes` fresh bytes, or zero while counting.
    fn take(&mut self, bytes: usize) -> usize {
        let at = self.used;
        self.used += bytes;
        if self.base == 0 {
            return 0;
        }
        assert!(
            self.used <= self.reserved,
            "a json native builds more than it counted"
        );
        self.base + at
    }

    /// A string holding `text`.
    pub(super) fn string(&mut self, text: &str) -> usize {
        let size = str_size(text.len());
        let string = self.take(size);
        if string != 0 {
            // SAFETY: the string has room for its two words and the text.
            // The last word first, for the zeros after the text.
            unsafe {
                store(string + size - WORD, 0);
                store(string, TYPE_STRING);
                store(
                    string + STRING_LENGTHS,
                    text.len() | (text.chars().count() << STRING_CHAR_COUNT_SHIFT),
                );
                std::ptr::copy_nonoverlapping(
                    text.as_ptr(),
                    (string + STRING_TEXT) as *mut u8,
                    text.len(),
                );
            }
        }
        string
    }

    /// A box of the type `type_id` holding `bits`.
    pub(super) fn boxed(&mut self, type_id: usize, bits: usize) -> usize {
        let object = self.take(BOX_SIZE);
        if object != 0 {
            // SAFETY: a box is a header and one value.
            unsafe {
                store(object, type_id);
                store(object + BOX_VALUE, bits);
            }
        }
        object
    }

    /// An array of `length` elements, which are `items` when building.
    pub(super) fn array(&mut self, length: usize, elements: Elements, items: &[usize]) -> usize {
        let array = self.take(array_size(length));
        if array != 0 {
            assert!(items.len() == length, "an array is built of its items");
            let (header, storage_type) = match elements {
                Elements::Refs => (TYPE_ARRAY_OF_REFS, TYPE_REFS),
                Elements::Floats => (TYPE_ARRAY_OF_VALUES | HEADER_FLOATS, TYPE_VALUES),
                Elements::Values => (TYPE_ARRAY_OF_VALUES, TYPE_VALUES),
            };
            // SAFETY: the array has room for itself and its storage.
            unsafe {
                store(array, header);
                store(array + ARRAY_LENGTH, length);
                if length == 0 {
                    store(array + ARRAY_DATA, 0);
                } else {
                    let storage = array + ARRAY_SIZE;
                    let data = storage + STORAGE_ELEMENTS;
                    store(storage, storage_type);
                    store(storage + STORAGE_CAPACITY, length);
                    std::ptr::copy_nonoverlapping(items.as_ptr(), data as *mut usize, length);
                    store(array + ARRAY_DATA, data);
                }
            }
        }
        array
    }

    /// A map of string keys with `count` entries, which are `entries` when
    /// building: each a key that holds its own text and a value. A key
    /// that comes twice keeps its first place and takes its last value.
    pub(super) fn map(
        &mut self,
        count: usize,
        values: Elements,
        entries: &[(usize, usize)],
    ) -> usize {
        let object = self.take(map::size_for(count));
        if object != 0 {
            assert!(entries.len() == count, "a map is built of its entries");
            // SAFETY: the map has the room `size_for` says, and its keys
            // are strings that hold their text.
            unsafe {
                map::new_at(object, true, matches!(values, Elements::Refs), count);
                if matches!(values, Elements::Floats) {
                    store(object, crate::heap::load(object) | HEADER_FLOATS);
                }
                for (key, value) in entries {
                    map::set_in_place(object, *key, *value);
                }
            }
        }
        object
    }

    /// An instance of `size` bytes of the type `type_id`, every slot zero.
    pub(super) fn instance(&mut self, type_id: usize, size: usize) -> usize {
        let object = self.take(size);
        if object != 0 {
            // SAFETY: the instance is `size` bytes.
            unsafe {
                std::ptr::write_bytes(object as *mut u8, 0, size);
                store(object, type_id);
            }
        }
        object
    }
}
