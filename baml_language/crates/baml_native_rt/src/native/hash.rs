//! `baml.hash`: the built-in hasher, whose state is plain data in the heap.

use super::{
    NativeBamlHashDefaultHasher, NativeBamlHashHasherForDefaultHasher, Natives, Obj, ObjRef,
    Opaque, Root, Scope,
};
use crate::layout::NAMED_DEFAULT_HASHER;

/// SipHash-1-3 with both keys zero, in progress: what the standard
/// library's `DefaultHasher` is when new, with a state that is `Copy`, so
/// that it lives in the heap and the collector moves it like any bytes.
#[derive(Clone, Copy)]
pub(crate) struct Sip13 {
    v0: u64,
    v1: u64,
    v2: u64,
    v3: u64,
    /// The bytes written that do not yet fill a word, low byte first.
    tail: u64,
    /// How many of them there are.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "read when bytes are written: see `Self::write`")
    )]
    pending: usize,
    /// How many bytes were written in all.
    length: usize,
}

impl Sip13 {
    pub(crate) const fn new() -> Self {
        Self {
            v0: 0x736f_6d65_7073_6575,
            v1: 0x646f_7261_6e64_6f6d,
            v2: 0x6c79_6765_6e65_7261,
            v3: 0x7465_6462_7974_6573,
            tail: 0,
            pending: 0,
            length: 0,
        }
    }

    fn round(&mut self) {
        self.v0 = self.v0.wrapping_add(self.v1);
        self.v1 = self.v1.rotate_left(13) ^ self.v0;
        self.v0 = self.v0.rotate_left(32);
        self.v2 = self.v2.wrapping_add(self.v3);
        self.v3 = self.v3.rotate_left(16) ^ self.v2;
        self.v0 = self.v0.wrapping_add(self.v3);
        self.v3 = self.v3.rotate_left(21) ^ self.v0;
        self.v2 = self.v2.wrapping_add(self.v1);
        self.v1 = self.v1.rotate_left(17) ^ self.v2;
        self.v2 = self.v2.rotate_left(32);
    }

    fn word(&mut self, word: u64) {
        self.v3 ^= word;
        self.round();
        self.v0 ^= word;
    }

    /// Append `bytes`. Writing a stream in pieces is writing it whole.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "`Hasher.write` takes a `uint8array`, which no native can be passed yet"
        )
    )]
    pub(crate) fn write(&mut self, bytes: &[u8]) {
        self.length += bytes.len();
        for &byte in bytes {
            self.tail |= u64::from(byte) << (8 * self.pending);
            self.pending += 1;
            if self.pending == 8 {
                let word = std::mem::take(&mut self.tail);
                self.word(word);
                self.pending = 0;
            }
        }
    }

    /// The hash of what was written so far. The state is left as it was.
    pub(crate) fn finish(mut self) -> u64 {
        let last = (self.length as u64 & 0xFF) << 56 | self.tail;
        self.word(last);
        self.v2 ^= 0xFF;
        for _ in 0..3 {
            self.round();
        }
        self.v0 ^ self.v1 ^ self.v2 ^ self.v3
    }
}

impl NativeBamlHashDefaultHasher for Natives {
    fn new<'s>(scope: &mut Scope<'s>) -> Root<'s, Obj> {
        let state = scope.alloc_plain(Sip13::new());
        let class = scope.named_class(NAMED_DEFAULT_HASHER);
        let mut hasher = scope.build(class);
        hasher.set(scope, 0, state);
        hasher.finish()
    }
}

impl NativeBamlHashHasherForDefaultHasher for Natives {
    fn finish(defaulthasher: ObjRef<'_>) -> i64 {
        // SAFETY: a `DefaultHasher` has one field, its state.
        let state = unsafe { defaulthasher.field::<Opaque>(0) }.plain::<Sip13>();
        // An arithmetic shift keeps the hash in the range of an `int` of
        // the interpreter, which is one bit narrower.
        state.get().finish().cast_signed() >> 1
    }
}

#[cfg(test)]
mod tests {
    use std::hash::Hasher as _;

    use super::Sip13;

    #[test]
    fn the_hasher_is_the_standard_librarys_in_any_pieces() {
        let bytes: Vec<u8> = (0..=255).cycle().take(1000).collect();
        for pieces in [1, 3, 7, 8, 9, 64, 1000] {
            let mut ours = Sip13::new();
            let mut theirs = std::collections::hash_map::DefaultHasher::new();
            assert_eq!(ours.finish(), theirs.finish());
            for piece in bytes.chunks(pieces) {
                ours.write(piece);
                theirs.write(piece);
                assert_eq!(ours.finish(), theirs.finish());
            }
        }
    }
}
