//! Where a tree is built: nowhere on the first pass, which only adds up
//! the bytes, and in reserved room on the second.

use crate::{
    heap::{Value, store},
    layout::{BOX_SIZE, BOX_VALUE, STRING_TEXT, TAG_BIGINT},
    map,
    native::{array_size, str_size},
};

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
        let string = self.take(str_size(text.len()));
        if string != 0 {
            // SAFETY: the string has room for its two words and the text.
            unsafe {
                crate::string::format(string, text.len(), text.chars().count());
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

    /// A `bigint` holding `value`.
    pub(super) fn bigint(&mut self, value: &num_bigint::BigInt) -> usize {
        let bytes = value.to_signed_bytes_le();
        let object = self.take(crate::bigint::size(&bytes));
        if object != 0 {
            // SAFETY: the object has the room `size` says.
            unsafe {
                store(object, TAG_BIGINT);
                crate::bigint::fill(object, &bytes);
            }
        }
        object
    }

    /// An array of the elements `items`, which are read only when
    /// building.
    pub(super) fn array(&mut self, items: impl ExactSizeIterator<Item = Value>) -> usize {
        let length = items.len();
        let array = self.take(array_size(length));
        if array != 0 {
            // SAFETY: the array has room for itself and its storage, and
            // nothing else reaches it yet.
            unsafe {
                crate::array::format(array, length);
                crate::array::fill(array, items);
            }
        }
        array
    }

    /// A map of string keys with the entries `entries`, which are read
    /// only when building: each a key that holds its own text and a
    /// value. A key that comes twice keeps its first place and takes its
    /// last value.
    pub(super) fn map(&mut self, entries: impl ExactSizeIterator<Item = (usize, Value)>) -> usize {
        let count = entries.len();
        let object = self.take(map::size_for(count));
        if object != 0 {
            // SAFETY: the map has the room `size_for` says, and its keys
            // are strings that hold their text.
            unsafe {
                map::new_at(object, count);
                for (key, value) in entries {
                    map::set_in_place(object, key, value);
                }
            }
        }
        object
    }

    /// An instance of `size` bytes of the type `type_id`, every slot
    /// `null`.
    pub(super) fn instance(&mut self, type_id: usize, size: usize) -> usize {
        let object = self.take(size);
        if object != 0 {
            // SAFETY: the instance is `size` bytes.
            unsafe { crate::class::format(object, type_id, size) };
        }
        object
    }
}
