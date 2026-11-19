//! What a standard library native is written against: typed views of heap
//! objects, roots that survive a collection, and the ways to make objects.
//!
//! The collector moves objects, so an address is good only until the next
//! allocation. The types here make the compiler hold a native to that:
//!
//! - A view ([`StrRef`], [`ArrRef`], [`ObjRef`]) borrows the [`Scope`]
//!   shared, and every allocation takes the scope exclusively. A view
//!   cannot be alive across an allocation.
//! - A [`Root`] is a slot the collector keeps current. It is what a native
//!   holds across allocations, and what an allocation returns.
//! - A [`Reservation`] is room for several objects taken at once. Nothing
//!   collects while it is open, so objects built in it need no roots and
//!   views read through it stay good until it is finished.
//!
//! A native that stops and goes on later is an `async fn`: see [`drive`].
//! Every way it can stop takes the scope exclusively too, so no view is
//! alive across one either.

#![allow(
    clippy::cast_possible_truncation,
    reason = "a word is 64 bits wherever the runtime is built"
)]

use std::{cell::Cell, marker::PhantomData, mem::MaybeUninit, ptr::NonNull};

use crate::{
    heap::{Mutator, load, store},
    layout::{
        ARRAY_DATA, ARRAY_LENGTH, ARRAY_SIZE, BYTES_DATA, BYTES_LENGTH, SLICE_MIN_BYTES,
        SLICE_OFFSET, SLICE_PARENT, SLICE_SIZE, STORAGE_CAPACITY, STORAGE_ELEMENTS,
        STRING_CHAR_COUNT_SHIFT, STRING_LENGTHS, STRING_TEXT, TYPE_ARRAY_OF_REFS,
        TYPE_ARRAY_OF_VALUES, TYPE_ID_MASK, TYPE_STRING, TYPE_STRING_SLICE, WORD, field_offset,
        instance_size, words,
    },
};

/// How many slots a chunk of roots has. A call that holds no more than
/// this many values at once uses the one chunk that is in place.
const ROOTS: usize = 16;

/// What a reference refers to: implemented by the markers [`Str`],
/// [`Arr`] and [`Obj`].
pub trait Managed {
    type View<'a>: View<'a>;
}

/// A typed, borrowed address of a live object.
pub trait View<'a>: Copy {
    /// # Safety
    ///
    /// `raw` is a live object of the view's type that does not move for
    /// `'a`.
    unsafe fn from_raw(raw: usize) -> Self;
    fn raw(self) -> usize;
}

/// What an array holds: [`Int`], [`Float`], [`Bool`], or anything
/// [`Managed`].
pub trait Element {
    type Value<'a>;
    /// Whether elements are references, which selects the array's type.
    const IS_REF: bool;
    /// # Safety
    ///
    /// `word` is an element of an array of this element type that does not
    /// move for `'a`.
    unsafe fn from_word<'a>(word: usize) -> Self::Value<'a>;
}

/// A string.
pub struct Str;
/// A `bigint`.
pub struct Big;
/// An array of `E`.
pub struct Arr<E>(PhantomData<E>);
/// A `uint8array`.
pub struct Bytes;
/// A class instance.
pub struct Obj;
/// A map, of whatever key and value types a call names.
pub struct Map;
/// An `int` array element.
pub struct Int;
/// A `float` array element.
pub struct Float;
/// A `bool` array element.
pub struct Bool;
/// Marks an array as holding values of a type parameter: whatever a call
/// names, each as the one word it is stored in. Whether the words are
/// references is the array's own type, so build one with
/// [`Reservation::slots`], not [`Reservation::array`].
pub struct Slot;

/// The type id a call passes for one of a builtin's type parameters.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TypeId(pub u32);

impl Managed for Str {
    type View<'a> = StrRef<'a>;
}
impl Managed for Big {
    type View<'a> = BigRef<'a>;
}
impl Managed for Bytes {
    type View<'a> = BytesRef<'a>;
}
impl<E: Element> Managed for Arr<E> {
    type View<'a> = ArrRef<'a, E>;
}
impl Managed for Obj {
    type View<'a> = ObjRef<'a>;
}
impl Managed for Map {
    type View<'a> = MapRef<'a>;
}

impl Element for Int {
    type Value<'a> = i64;
    const IS_REF: bool = false;
    unsafe fn from_word<'a>(word: usize) -> Self::Value<'a> {
        word.cast_signed() as i64
    }
}
impl Element for Float {
    type Value<'a> = f64;
    const IS_REF: bool = false;
    unsafe fn from_word<'a>(word: usize) -> Self::Value<'a> {
        f64::from_bits(word as u64)
    }
}
impl Element for Bool {
    type Value<'a> = bool;
    const IS_REF: bool = false;
    unsafe fn from_word<'a>(word: usize) -> Self::Value<'a> {
        word != 0
    }
}
impl Element for Slot {
    type Value<'a> = usize;
    const IS_REF: bool = false;
    unsafe fn from_word<'a>(word: usize) -> Self::Value<'a> {
        word
    }
}
impl<M: Managed> Element for M {
    type Value<'a> = M::View<'a>;
    const IS_REF: bool = true;
    unsafe fn from_word<'a>(word: usize) -> Self::Value<'a> {
        // SAFETY: the caller's contract.
        unsafe { M::View::from_raw(word) }
    }
}

/// A view of a `bigint`.
#[derive(Clone, Copy)]
pub struct BigRef<'a>(usize, PhantomData<&'a ()>);

impl<'a> View<'a> for BigRef<'a> {
    unsafe fn from_raw(raw: usize) -> Self {
        Self(raw, PhantomData)
    }
    fn raw(self) -> usize {
        self.0
    }
}

impl BigRef<'_> {
    /// The value, read out of the object.
    pub fn value(self) -> num_bigint::BigInt {
        // SAFETY: a view is of a live object of its type.
        unsafe { crate::bigint::value(self.0) }
    }
}

/// A view of a string.
#[derive(Clone, Copy)]
pub struct StrRef<'a>(usize, PhantomData<&'a ()>);

impl<'a> View<'a> for StrRef<'a> {
    unsafe fn from_raw(raw: usize) -> Self {
        Self(raw, PhantomData)
    }
    fn raw(self) -> usize {
        self.0
    }
}

impl<'a> StrRef<'a> {
    pub fn as_str(self) -> &'a str {
        // SAFETY: a view is of a live string that does not move for `'a`.
        unsafe { crate::string::text(self.0) }
    }

    /// How many bytes of text the string has, which it keeps: no need to
    /// find the text for it.
    pub fn byte_len(self) -> usize {
        // SAFETY: the length is the low half of a string's second word.
        unsafe { load(self.0 + STRING_LENGTHS) & 0xFFFF_FFFF }
    }

    /// The string as what parts are taken of: its text found once.
    pub fn whole(self) -> Whole<'a> {
        // SAFETY: a view is of a live string, and a slice is of a string
        // that holds its text.
        let (holder, start) = unsafe {
            if load(self.0) & TYPE_ID_MASK == TYPE_STRING {
                (self.0, 0)
            } else {
                (load(self.0 + SLICE_PARENT), load(self.0 + SLICE_OFFSET))
            }
        };
        Whole {
            text: self.as_str(),
            holder,
            start,
        }
    }

    /// How many characters the string has, which it keeps.
    pub fn char_count(self) -> usize {
        // SAFETY: the count is the high half of a string's second word.
        unsafe { load(self.0 + STRING_LENGTHS) >> STRING_CHAR_COUNT_SHIFT }
    }

    /// The string, to finish the reservation it was built in with.
    pub fn keep(self) -> Built<Str> {
        Built(self.0, PhantomData)
    }

    /// Whether every character is one byte, so that a character's position
    /// is its byte's.
    pub fn is_ascii(self) -> bool {
        // SAFETY: the byte length and the character count are the halves
        // of a string's second word.
        let lengths = unsafe { load(self.0 + STRING_LENGTHS) };
        lengths & 0xFFFF_FFFF == lengths >> STRING_CHAR_COUNT_SHIFT
    }
}

/// A string whose parts are being taken: its text, the string that holds
/// that text, and the byte of the holder's text it starts at.
#[derive(Clone, Copy)]
pub struct Whole<'a> {
    text: &'a str,
    holder: usize,
    start: usize,
}

impl<'a> Whole<'a> {
    pub fn as_str(self) -> &'a str {
        self.text
    }
}

/// A view of an array of `E`.
pub struct ArrRef<'a, E>(usize, PhantomData<(&'a (), E)>);

impl<E> Clone for ArrRef<'_, E> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<E> Copy for ArrRef<'_, E> {}

impl<'a, E: Element> View<'a> for ArrRef<'a, E> {
    unsafe fn from_raw(raw: usize) -> Self {
        Self(raw, PhantomData)
    }
    fn raw(self) -> usize {
        self.0
    }
}

impl<'a, E: Element> ArrRef<'a, E> {
    pub fn len(self) -> usize {
        // SAFETY: a view is of a live array.
        unsafe { load(self.0 + ARRAY_LENGTH) }
    }

    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// The elements, as the words they are stored in.
    fn words(self) -> &'a [usize] {
        let length = self.len();
        if length == 0 {
            return &[];
        }
        // SAFETY: an array that has elements has storage holding `length`
        // of them at its data address, and neither moves for `'a`.
        unsafe { std::slice::from_raw_parts(load(self.0 + ARRAY_DATA) as *const usize, length) }
    }

    pub fn get(self, index: usize) -> Option<E::Value<'a>> {
        // SAFETY: the word is an element of this array.
        self.words()
            .get(index)
            .map(|&word| unsafe { E::from_word(word) })
    }

    pub fn iter(self) -> impl ExactSizeIterator<Item = E::Value<'a>> {
        // SAFETY: each word is an element of this array.
        self.words()
            .iter()
            .map(|&word| unsafe { E::from_word(word) })
    }
}

impl ArrRef<'_, Slot> {
    /// Whether the elements are references.
    pub fn holds_refs(self) -> bool {
        // SAFETY: a view is of a live array.
        unsafe { load(self.0) & TYPE_ID_MASK == TYPE_ARRAY_OF_REFS }
    }
}

impl<'a> ArrRef<'a, Int> {
    pub fn as_slice(self) -> &'a [i64] {
        let words = self.words();
        // SAFETY: an `int` element is the word itself.
        unsafe { std::slice::from_raw_parts(words.as_ptr().cast(), words.len()) }
    }
}

impl<'a> ArrRef<'a, Float> {
    pub fn as_slice(self) -> &'a [f64] {
        let words = self.words();
        // SAFETY: a `float` element is the word's bits.
        unsafe { std::slice::from_raw_parts(words.as_ptr().cast(), words.len()) }
    }
}

/// A view of a `uint8array`.
///
/// The array can be written, by compiled code and by natives, so its bytes
/// are reached here through a raw address and a length, and read and
/// written by copying: [`Self::get`], [`Self::to_vec`], [`Self::copy_to`],
/// [`Self::copy_from`], [`Self::copy_from_bytes`]. A native that stops
/// takes them with [`Self::hold`]. The two methods that lend the bytes as
/// a Rust slice instead, [`Self::as_slice`] and [`Self::as_mut_slice`], are
/// the only places a slice is made of bytes in the heap, and both rely on a
/// single worker.
#[derive(Clone, Copy)]
pub struct BytesRef<'a>(usize, PhantomData<&'a ()>);

impl<'a> View<'a> for BytesRef<'a> {
    unsafe fn from_raw(raw: usize) -> Self {
        Self(raw, PhantomData)
    }
    fn raw(self) -> usize {
        self.0
    }
}

impl<'a> BytesRef<'a> {
    pub fn len(self) -> usize {
        // SAFETY: a view is of a live `uint8array`.
        unsafe { load(self.0 + BYTES_LENGTH) }
    }

    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// The address of the first byte: null while the array has never held
    /// one.
    fn data(self) -> *mut u8 {
        // SAFETY: a view is of a live `uint8array`.
        unsafe { load(self.0 + BYTES_DATA) as *mut u8 }
    }

    /// The byte at `index`.
    pub fn get(self, index: usize) -> Option<u8> {
        // SAFETY: the storage holds the length in bytes, checked here.
        (index < self.len()).then(|| unsafe { self.data().add(index).read() })
    }

    /// A copy of the bytes: what a native that stops takes with it, and
    /// what any native reads the whole array by.
    pub fn to_vec(self) -> Vec<u8> {
        let length = self.len();
        let mut copy = Vec::with_capacity(length);
        // SAFETY: the storage holds `length` bytes, the vector has room
        // for them, and every byte is initialized.
        unsafe {
            if length != 0 {
                std::ptr::copy_nonoverlapping(self.data(), copy.as_mut_ptr(), length);
            }
            copy.set_len(length);
        }
        copy
    }

    /// The bytes, for a native to take across a stop: the job of
    /// `scope.io`, the work of `scope.off_heap`, or its own code after an
    /// `await`. Every native that stops with an array's bytes takes them
    /// here and nowhere else.
    ///
    /// Today this is a copy, so what is written into the array during the
    /// stop is not seen. It is the one place that changes when a stop can
    /// instead keep the array's storage block alive and in place: what is
    /// held is the storage's bytes as they are now, never the array, whose
    /// storage a `push` replaces.
    pub fn hold(self) -> HeldBytes {
        HeldBytes(self.to_vec())
    }

    /// Copy the bytes from `from` on into `into`, all of it.
    pub fn copy_to(self, from: usize, into: &mut [u8]) {
        assert!(
            from <= self.len() && into.len() <= self.len() - from,
            "past the uint8array's end"
        );
        // SAFETY: the range is inside the array, checked here, and `into`
        // is not in the heap.
        unsafe {
            if !into.is_empty() {
                std::ptr::copy_nonoverlapping(self.data().add(from), into.as_mut_ptr(), into.len());
            }
        }
    }

    /// Write `bytes` over the array's from `at` on.
    pub fn copy_from(self, at: usize, bytes: &[u8]) {
        assert!(
            at <= self.len() && bytes.len() <= self.len() - at,
            "past the uint8array's end"
        );
        // SAFETY: the range is inside the array, checked here, and `bytes`
        // is not in the heap.
        unsafe {
            if !bytes.is_empty() {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.data().add(at), bytes.len());
            }
        }
    }

    /// Write `count` bytes of `source`, from `from` on, over the array's
    /// from `at` on. The two may be the same array.
    pub fn copy_from_bytes(self, at: usize, source: BytesRef<'_>, from: usize, count: usize) {
        assert!(
            at <= self.len()
                && count <= self.len() - at
                && from <= source.len()
                && count <= source.len() - from,
            "past a uint8array's end"
        );
        // SAFETY: both ranges are inside their arrays, checked here.
        unsafe {
            if count != 0 {
                std::ptr::copy(source.data().add(from), self.data().add(at), count);
            }
        }
    }

    /// Remove the last byte and return it.
    pub fn pop(self) -> Option<u8> {
        let last = self.len().checked_sub(1)?;
        let byte = self.get(last);
        // SAFETY: a view is of a live `uint8array`, which keeps its
        // storage.
        unsafe { store(self.0 + BYTES_LENGTH, last) };
        byte
    }

    /// The bytes, lent in place: for a native that only scans them and
    /// would otherwise copy the whole array to do it.
    ///
    /// This is the one place a shared slice is made of bytes in the heap.
    /// It relies on a single worker: nothing else runs while a native
    /// that has not stopped does, so nothing writes the bytes while the
    /// slice is alive. With several workers on one heap another could,
    /// and every caller of this has to be looked at again then.
    pub fn as_slice(self) -> &'a [u8] {
        match self.len() {
            0 => &[],
            // SAFETY: the storage holds `length` bytes, which do not move
            // for `'a`, and see above for why nothing writes them.
            length => unsafe { std::slice::from_raw_parts(self.data(), length) },
        }
    }

    /// The bytes, lent in place to change.
    ///
    /// This is the one place an exclusive slice is made of bytes in the
    /// heap. It relies on a single worker, as [`Self::as_slice`] does.
    ///
    /// # Safety
    ///
    /// No other slice of this array is alive while this one is.
    pub unsafe fn as_mut_slice(self) -> &'a mut [u8] {
        match self.len() {
            0 => &mut [],
            // SAFETY: as in `Self::as_slice`, and the caller's contract.
            length => unsafe { std::slice::from_raw_parts_mut(self.data(), length) },
        }
    }
}

/// The bytes of a `uint8array` as a native holds them across a stop: see
/// [`BytesRef::hold`]. It is `'static` and `Send`, so a job can own it.
pub struct HeldBytes(Vec<u8>);

impl std::ops::Deref for HeldBytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.0
    }
}

/// A view of a map. What is done with one is in [`crate::map`].
#[derive(Clone, Copy)]
pub struct MapRef<'a>(usize, PhantomData<&'a ()>);

impl<'a> View<'a> for MapRef<'a> {
    unsafe fn from_raw(raw: usize) -> Self {
        Self(raw, PhantomData)
    }
    fn raw(self) -> usize {
        self.0
    }
}

/// A view of a class instance: its slots, by position.
#[derive(Clone, Copy)]
pub struct ObjRef<'a>(usize, PhantomData<&'a ()>);

impl<'a> View<'a> for ObjRef<'a> {
    unsafe fn from_raw(raw: usize) -> Self {
        Self(raw, PhantomData)
    }
    fn raw(self) -> usize {
        self.0
    }
}

impl<'a> ObjRef<'a> {
    /// The word in slot `index`.
    ///
    /// # Safety
    ///
    /// The instance has that slot.
    pub unsafe fn slot(self, index: usize) -> usize {
        // SAFETY: the caller's contract.
        unsafe { load(self.0 + field_offset(index)) }
    }

    /// The object slot `index` refers to.
    ///
    /// # Safety
    ///
    /// The instance has that slot, and it holds a reference to an `M` that
    /// is not `null`.
    pub unsafe fn field<M: Managed>(self, index: usize) -> M::View<'a> {
        // SAFETY: the caller's contract.
        unsafe { M::View::from_raw(self.slot(index)) }
    }
}

/// An optional `int`, `float` or `bool` as compiled code holds it: the
/// value's bits, and whether there is a value at all.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct OptionalScalar {
    pub payload: u64,
    pub present: u64,
}

impl From<Option<i64>> for OptionalScalar {
    fn from(value: Option<i64>) -> Self {
        Self {
            payload: value.unwrap_or_default().cast_unsigned(),
            present: u64::from(value.is_some()),
        }
    }
}

impl From<Option<f64>> for OptionalScalar {
    fn from(value: Option<f64>) -> Self {
        Self {
            payload: value.unwrap_or_default().to_bits(),
            present: u64::from(value.is_some()),
        }
    }
}

/// An optional value of a type parameter: its word, and whether there is
/// one. The word of an absent reference is zero.
impl From<Option<usize>> for OptionalScalar {
    fn from(value: Option<usize>) -> Self {
        Self {
            payload: value.unwrap_or_default() as u64,
            present: u64::from(value.is_some()),
        }
    }
}

impl From<Option<bool>> for OptionalScalar {
    fn from(value: Option<bool>) -> Self {
        Self {
            payload: u64::from(value.unwrap_or_default()),
            present: u64::from(value.is_some()),
        }
    }
}

/// Some of the slots behind the roots of one call: [`ROOTS`] of them, and
/// which are roots.
struct Chunk {
    slots: MaybeUninit<[Cell<usize>; ROOTS]>,
    /// Which of the slots are roots, a bit each. The bit of a slot that is
    /// not held is clear.
    refs: Cell<u32>,
    /// The chunk the next slots are in, once a call has held that many:
    /// on the Rust heap, and null until then.
    next: Cell<*mut Chunk>,
}

const _: () = assert!(ROOTS <= u32::BITS as usize);

impl Chunk {
    fn new() -> Self {
        Self {
            slots: MaybeUninit::uninit(),
            refs: Cell::new(0),
            next: Cell::new(std::ptr::null_mut()),
        }
    }
}

/// The slots behind the roots of one call of a native. It lives in the
/// frame of the function compiled code called, or for an `async` native in
/// the block that holds its state.
///
/// The first [`ROOTS`] slots are here. A call that holds more gets further
/// chunks of as many, which stay where they are until the call is over: a
/// root is the address of its slot. A slot is written when it is first
/// handed out and not before: a call that roots two things pays for two.
pub struct RootSlots {
    first: Chunk,
    /// How many slots are held.
    used: Cell<usize>,
    /// The chunk after the first that a slot was last taken from, and
    /// which chunk it is, counting the first as zero: the one a call that
    /// holds many asks for next. Zero and null until there is one.
    last: Cell<(usize, *mut Chunk)>,
}

impl RootSlots {
    pub fn new() -> Self {
        Self {
            first: Chunk::new(),
            used: Cell::new(0),
            last: Cell::new((0, std::ptr::null_mut())),
        }
    }

    /// Chunk `number`, which is not the first, made if no call has held
    /// that many slots yet.
    #[cold]
    fn further(&self, number: usize) -> &Chunk {
        let (mut at, mut chunk) = match self.last.get() {
            (at, chunk) if at != 0 && at <= number => (at, chunk.cast_const()),
            _ => (0, &raw const self.first),
        };
        // SAFETY: a chunk's `next` is null or a chunk made here, which
        // lives until `self` is dropped.
        unsafe {
            while at < number {
                let mut next = (*chunk).next.get();
                if next.is_null() {
                    next = Box::into_raw(Box::new(Chunk::new()));
                    (*chunk).next.set(next);
                }
                (chunk, at) = (next, at + 1);
            }
            self.last.set((number, chunk.cast_mut()));
            &*chunk
        }
    }

    /// The next slot, holding `word`, which is a reference if `is_ref`.
    fn hold(&self, word: usize, is_ref: bool) -> &Cell<usize> {
        let index = self.used.get();
        self.used.set(index + 1);
        let chunk = if index < ROOTS {
            &self.first
        } else {
            self.further(index / ROOTS)
        };
        let at = index % ROOTS;
        chunk.refs.set(chunk.refs.get() | u32::from(is_ref) << at);
        // SAFETY: the slot is inside the chunk's array, a cell may be
        // written through a shared reference, and it is written before the
        // reference to it is made.
        unsafe {
            let slot = chunk
                .slots
                .as_ptr()
                .cast::<Cell<usize>>()
                .add(at)
                .cast_mut();
            slot.write(Cell::new(word));
            &*slot
        }
    }

    /// Give back every slot from `kept` on, and say how many of them were
    /// roots.
    fn release_to(&self, kept: usize) -> usize {
        let mut dropped = 0;
        let mut chunk = &raw const self.first;
        let mut first_slot = 0;
        // SAFETY: a call that holds slots has the chunks they are in.
        unsafe {
            while first_slot < self.used.get() {
                let refs = (*chunk).refs.get();
                // The slots of this chunk that stay held.
                let stay = match kept.saturating_sub(first_slot) {
                    stay if stay >= ROOTS => u32::MAX,
                    stay => (1 << stay) - 1,
                };
                dropped += (refs & !stay).count_ones() as usize;
                (*chunk).refs.set(refs & stay);
                first_slot += ROOTS;
                chunk = (*chunk).next.get();
            }
        }
        self.used.set(kept);
        dropped
    }

    /// Hand `each` the address of every slot that is a root.
    pub(crate) fn each_root(&self, mut each: impl FnMut(usize)) {
        let mut chunk = &raw const self.first;
        let mut left = self.used.get();
        // SAFETY: as in `Self::release_to`.
        unsafe {
            while left != 0 {
                let refs = (*chunk).refs.get();
                let first = (*chunk).slots.as_ptr() as usize;
                for at in 0..left.min(ROOTS) {
                    if refs >> at & 1 != 0 {
                        each(first + at * WORD);
                    }
                }
                left = left.saturating_sub(ROOTS);
                chunk = (*chunk).next.get();
            }
        }
    }
}

impl Default for RootSlots {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for RootSlots {
    fn drop(&mut self) {
        let mut chunk = self.first.next.get();
        while !chunk.is_null() {
            // SAFETY: made by `Self::further`, and no root of the call is
            // used once its slots are dropped.
            chunk = unsafe { Box::from_raw(chunk) }.next.get();
        }
    }
}

/// A reference the collector keeps current, for as long as the call it was
/// made in.
pub struct Root<'s, M>(&'s Cell<usize>, PhantomData<M>);

// SAFETY: a root names a slot of its scope's, and the slot is read and
// written only by the thread running that scope's native and by a
// collection, which runs while that thread is stopped. A root crosses
// threads only inside the state of an `async` native, which moves whole
// and between polls. Nothing a native hands another thread can hold one:
// the job of `Scope::io` and the work of `Scope::off_heap` are `'static`,
// and a root is not.
unsafe impl<M> Send for Root<'_, M> {}

impl<M> Clone for Root<'_, M> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<M> Copy for Root<'_, M> {}

impl<'s, M: Managed> Root<'s, Arr<M>> {
    /// Put `value`, or `null`, at `index` of the array.
    pub fn set(self, scope: &mut Scope<'s>, index: usize, value: Option<Root<'s, M>>) {
        let array = self.raw(scope);
        // SAFETY: a root holds a live array, which has storage for its
        // length, checked here.
        unsafe {
            assert!(index < load(array + ARRAY_LENGTH), "past the array's end");
            store(
                load(array + ARRAY_DATA) + index * WORD,
                value.map_or(0, |value| value.raw(scope)),
            );
        }
    }
}

impl<M: Managed> Root<'_, M> {
    /// The object, for as long as `scope` allocates nothing.
    pub fn get<'a>(self, _scope: &'a Scope<'_>) -> M::View<'a> {
        // SAFETY: the slot holds a live `M`, which moves only when the
        // scope is borrowed exclusively.
        unsafe { M::View::from_raw(self.0.get()) }
    }

    /// The object's address, to hand back to compiled code: good until
    /// `scope` allocates or stops, like a view.
    ///
    /// It takes the scope so that the root is read only when nothing else
    /// holds the scope. A stop holds it, so no address is read during one:
    ///
    /// ```compile_fail,E0502
    /// use baml_native_rt::native::{Root, Scope, Str};
    ///
    /// async fn read_during_a_stop<'s>(scope: &mut Scope<'s>, text: Root<'s, Str>) -> usize {
    ///     let waiting = scope.io(async {});
    ///     let address = text.raw(scope);
    ///     waiting.await;
    ///     address
    /// }
    /// ```
    ///
    /// Read once the stop is over, it is the address the object has then:
    ///
    /// ```no_run
    /// use baml_native_rt::native::{Root, Scope, Str};
    ///
    /// async fn read_after_a_stop<'s>(scope: &mut Scope<'s>, text: Root<'s, Str>) -> usize {
    ///     scope.io(async {}).await;
    ///     text.raw(scope)
    /// }
    /// ```
    pub fn raw(self, _scope: &Scope<'_>) -> usize {
        self.0.get()
    }
}

/// The error a native throws: the object compiled code receives, and what
/// it was caught with when compiled code threw it first.
pub struct Thrown<'s> {
    /// An instance of one of the standard library's error classes, or
    /// whatever the BAML a native called threw, `null` included.
    error: Elem<'s>,
    /// The trace it was caught with, to throw it again with.
    context: Option<Elem<'s>>,
}

/// A value of a type parameter held across allocations: a root when the
/// type's values are references, the plain word otherwise.
#[derive(Clone, Copy)]
pub struct Elem<'s> {
    slot: &'s Cell<usize>,
    is_ref: bool,
}

// SAFETY: as for `Root`.
unsafe impl Send for Elem<'_> {}

impl Elem<'_> {
    /// The value's word, current until the next allocation.
    pub fn get(self, _scope: &Scope<'_>) -> usize {
        self.slot.get()
    }

    pub fn is_ref(self) -> bool {
        self.is_ref
    }
}

/// What a native that can throw returns to compiled code, in two registers:
/// the value's bits, and the thrown object or zero.
#[repr(C)]
pub struct RtResult {
    pub value: u64,
    pub thrown: usize,
}

impl RtResult {
    pub fn ok(value: u64) -> Self {
        Self { value, thrown: 0 }
    }

    pub fn thrown(thrown: &Thrown<'_>) -> Self {
        Self {
            value: 0,
            thrown: thrown.error.slot.get(),
        }
    }
}

/// One call of a native: the mutator it allocates with and the roots it
/// holds.
pub struct Scope<'s> {
    /// The worker running the native. Compiled code a native calls uses
    /// the worker too, so the scope holds no reference to it.
    mutator: NonNull<Mutator>,
    slots: &'s RootSlots,
    /// Where an `async` native leaves what it stops for, and finds the
    /// answer: null for any other.
    exchange: *mut drive::Exchange,
    life: PhantomData<&'s mut Mutator>,
}

// SAFETY: a scope is used by one thread at a time: the one running its
// native. It crosses threads only inside the state of an `async` native,
// which moves whole and between polls, to the worker that polls it next.
// It is not `Sync`, so no other thread can read a root through it.
unsafe impl Send for Scope<'_> {}

impl<'s> Scope<'s> {
    fn mutator(&mut self) -> &mut Mutator {
        // SAFETY: the worker outlives the call, and nothing else uses it
        // while the native runs.
        unsafe { self.mutator.as_mut() }
    }

    /// Run `run` where it has room to recurse: on the worker's own stack
    /// when a task is running, whose stack keeps only a little for the
    /// runtime. `run` calls no compiled code.
    pub(crate) fn with_room<R>(&mut self, run: impl FnOnce(&mut Self) -> R) -> R {
        let Some(stack) = self.mutator().worker_stack() else {
            return run(self);
        };
        self.mutator().leave_task_stack(true);
        // SAFETY: a task is running, and the worker's stack is free below
        // where it left it.
        let result = unsafe { crate::sched::on_stack(stack, || run(self)) };
        self.mutator().leave_task_stack(false);
        result
    }

    /// The address of the program's type table.
    pub(crate) fn type_table(&self) -> usize {
        // SAFETY: as in `Self::mutator`.
        unsafe { self.mutator.as_ref() }.types()
    }

    /// A scope whose roots are in `slots`, for a call that returns before
    /// its task runs anything else.
    ///
    /// # Safety
    ///
    /// The mutator has been told the frame compiled code called
    /// ([`Mutator::enter`]).
    pub unsafe fn new(mutator: &'s mut Mutator, slots: &'s RootSlots) -> Self {
        Self {
            mutator: NonNull::from(mutator),
            slots,
            exchange: std::ptr::null_mut(),
            life: PhantomData,
        }
    }

    /// Root the object at `raw`.
    ///
    /// # Safety
    ///
    /// `raw` is a live `M`.
    pub unsafe fn root_raw<M: Managed>(&mut self, raw: usize) -> Root<'s, M> {
        Root(self.hold(raw, true), PhantomData)
    }

    /// Hold `word`, a value of the type a call names by `ty`.
    ///
    /// # Safety
    ///
    /// `word` is a value of that type: a live object or zero when the
    /// type's values are references.
    pub unsafe fn elem_raw(&mut self, word: usize, ty: TypeId) -> Elem<'s> {
        let is_ref = self.is_reference(ty);
        // The slot is this value's whether or not it is a root: a native
        // can hold two values that are not references.
        Elem {
            slot: self.hold(word, is_ref),
            is_ref,
        }
    }

    /// The next slot, holding `word`. The collector finds the roots of an
    /// `async` native through its task, and any other's in the mutator's
    /// own list.
    fn hold(&mut self, word: usize, is_ref: bool) -> &'s Cell<usize> {
        let slot = self.slots.hold(word, is_ref);
        if is_ref && self.exchange.is_null() {
            self.mutator().root_at(slot.as_ptr() as usize);
        }
        slot
    }

    /// Give back the last `count` slots handed out, for a native that
    /// holds one value after another in a loop and stops in it. A loop
    /// that does not stop uses [`Self::scoped`], which needs no promise.
    ///
    /// # Safety
    ///
    /// No root or [`Elem`] of those slots is used again.
    pub(crate) unsafe fn release_last(&mut self, count: usize) {
        let dropped = self.slots.release_to(self.slots.used.get() - count);
        if self.exchange.is_null() {
            self.mutator().unroot(dropped);
        }
    }

    /// Run `body` with a scope of its own, and give back every root it
    /// made when it returns: a loop that makes a few each time around then
    /// holds one round's and no more.
    ///
    /// The roots `body` makes cannot leave it: they are of a lifetime only
    /// `body` sees, so neither what it returns nor anything outside it can
    /// hold one. What it makes and wants kept it stores in an object the
    /// outer scope holds, whose roots it can use: the second argument is
    /// what tells the compiler they outlive its own. `body` does not stop,
    /// since it is no `async` block.
    ///
    /// ```no_run
    /// use baml_native_rt::native::{Root, Scope, Str};
    ///
    /// fn measures<'s>(scope: &mut Scope<'s>, held: Root<'s, Str>) -> usize {
    ///     scope.scoped(|scope, _| {
    ///         let made = scope.alloc_str("made inside");
    ///         made.get(scope).byte_len() + held.get(scope).byte_len()
    ///     })
    /// }
    /// ```
    ///
    /// A root made inside does not come out:
    ///
    /// ```compile_fail
    /// use baml_native_rt::native::{Root, Scope, Str};
    ///
    /// fn leaks<'s>(scope: &mut Scope<'s>) -> Root<'s, Str> {
    ///     scope.scoped(|scope, _| scope.alloc_str("made inside"))
    /// }
    /// ```
    pub fn scoped<R>(
        &mut self,
        body: impl for<'n> FnOnce(&mut Scope<'n>, Within<'n, 's>) -> R,
    ) -> R {
        let held = self.slots.used.get();
        let mut inner = Scope {
            mutator: self.mutator,
            slots: self.slots,
            exchange: self.exchange,
            life: PhantomData,
        };
        let result = body(&mut inner, Within(PhantomData));
        // Dropping it would give back the roots of the whole call.
        std::mem::forget(inner);
        // SAFETY: every root made since is of the inner scope's lifetime,
        // which is over.
        unsafe { self.release_last(self.slots.used.get() - held) };
        result
    }

    /// Whether values of the type a call names by `ty` are references.
    pub fn is_reference(&self, ty: TypeId) -> bool {
        // SAFETY: as in `Self::mutator`.
        unsafe { self.mutator.as_ref() }.is_reference(ty.0 as usize)
    }

    /// A new string holding `text`. May collect.
    pub fn alloc_str(&mut self, text: &str) -> Root<'s, Str> {
        let string = crate::string::from_str(self.mutator(), text);
        // SAFETY: just made.
        unsafe { self.root_raw(string) }
    }

    /// A new `uint8array` holding a copy of `bytes`. May collect.
    pub fn alloc_bytes(&mut self, bytes: &[u8]) -> Root<'s, Bytes> {
        let object = crate::bytes::make(self.mutator(), bytes);
        // SAFETY: just made.
        unsafe { self.root_raw(object) }
    }

    /// A new `uint8array` of `length` zero bytes, to be written through a
    /// view. May collect.
    pub fn alloc_zeroed_bytes(&mut self, length: usize) -> Root<'s, Bytes> {
        let object = crate::bytes::zeroed(self.mutator(), length);
        // SAFETY: just made.
        unsafe { self.root_raw(object) }
    }

    /// Append `byte` to `bytes` and return the new length. May collect.
    pub fn push_byte(&mut self, bytes: Root<'s, Bytes>, byte: u8) -> usize {
        let object = bytes.raw(self);
        // SAFETY: a root holds a live `uint8array`.
        unsafe { crate::bytes::push(self.mutator(), object, byte) }
    }

    /// A new `bigint` holding `value`, or the `baml.panics.AllocFailure` of
    /// one past the limit. May collect.
    pub fn alloc_big(&mut self, value: &num_bigint::BigInt) -> Result<Root<'s, Big>, Thrown<'s>> {
        match crate::bigint::checked(self.mutator(), value) {
            // SAFETY: just made.
            Ok(object) => Ok(unsafe { self.root_raw(object) }),
            // SAFETY: just made.
            Err(thrown) => Err(unsafe { self.thrown(thrown) }),
        }
    }

    /// A new `bigint` holding `value`, which is known to be within the
    /// limit. May collect.
    pub fn make_big(&mut self, value: &num_bigint::BigInt) -> Root<'s, Big> {
        let object = crate::bigint::make(self.mutator(), value);
        // SAFETY: just made.
        unsafe { self.root_raw(object) }
    }

    /// Throw `baml.panics.AllocFailure`.
    pub fn alloc_failure(&mut self, message: &str) -> Thrown<'s> {
        let thrown = crate::bigint::alloc_failure(self.mutator(), message);
        // SAFETY: just made.
        unsafe { self.thrown(thrown) }
    }

    /// A new array of `length` elements, each zero: `0`, `0.0`, `false`, or
    /// no object. May collect.
    pub fn alloc_array<E: Element>(&mut self, length: usize) -> Root<'s, Arr<E>> {
        let reservation = self.reserve(array_size(length));
        let array = reservation.array::<E>(length).keep();
        reservation.finish(array)
    }

    /// A new instance of the class with the type id `type_id` and `slots`
    /// slots, each zero. May collect.
    ///
    /// # Safety
    ///
    /// `type_id` is the id of a class with that many slots, each of which
    /// may hold zero.
    pub unsafe fn alloc_instance(&mut self, type_id: usize, slots: usize) -> Root<'s, Obj> {
        let size = instance_size(slots);
        let object = self.mutator().allocate_object(type_id, size);
        // SAFETY: the object has `slots` words after its header.
        unsafe {
            std::ptr::write_bytes((object + field_offset(0)) as *mut usize, 0, slots);
            self.root_raw(object)
        }
    }

    /// Room for objects of `bytes` bytes in all ([`str_size`],
    /// [`array_size`]). May collect, once, now.
    pub fn reserve(&mut self, bytes: usize) -> Reservation<'_, 's> {
        let start = self.mutator().allocate(bytes);
        Reservation {
            scope: self,
            next: Cell::new(start),
            end: start + bytes,
        }
    }

    /// Throw `baml.errors.InvalidArgument`.
    pub fn invalid_argument(&mut self, message: &str) -> Thrown<'s> {
        self.error(crate::layout::WELL_KNOWN_INVALID_ARGUMENT, message)
    }

    /// Throw `baml.errors.Io`.
    pub fn io_error(&mut self, message: &str) -> Thrown<'s> {
        self.error(crate::layout::WELL_KNOWN_IO, message)
    }

    /// Throw `baml.errors.ParseError`.
    pub fn parse_error(&mut self, message: &str) -> Thrown<'s> {
        self.error(crate::layout::WELL_KNOWN_PARSE_ERROR, message)
    }

    /// An instance of the well-known error class `class`, whose one field
    /// is its message.
    fn error(&mut self, class: usize, message: &str) -> Thrown<'s> {
        self.error_of(class, &[message])
    }

    /// An instance of the well-known error class `class`, whose fields are
    /// these strings, in declaration order.
    pub(crate) fn error_of(&mut self, class: usize, fields: &[&str]) -> Thrown<'s> {
        let mut texts = [crate::class::Init::Int(0); crate::class::MOST_FIELDS];
        for (text, field) in texts.iter_mut().zip(fields) {
            *text = crate::class::Init::Text(field);
        }
        let error = crate::class::well_known(self.mutator(), class, &texts[..fields.len()]);
        // SAFETY: just made.
        unsafe { self.thrown(error) }
    }

    /// Throw `baml.panics.UserPanic`.
    pub fn user_panic(&mut self, message: &str) -> Thrown<'s> {
        self.error(crate::layout::WELL_KNOWN_USER_PANIC, message)
    }

    /// Throw `error`, an instance a native built.
    pub fn throw(&mut self, error: Root<'s, Obj>) -> Thrown<'s> {
        let error = error.raw(self);
        // SAFETY: a root holds a live object.
        unsafe { self.thrown(error) }
    }

    /// A new map of string keys to objects, with room for `count` keys.
    /// May collect.
    pub fn alloc_str_map(&mut self, count: usize) -> Root<'s, Map> {
        let map = crate::map::new(self.mutator(), true, true, count);
        // SAFETY: just made.
        unsafe { self.root_raw(map) }
    }

    /// Set `key` to `value`, or to `null`, in a map of string keys to
    /// objects. May collect.
    pub fn str_map_set<M: Managed>(
        &mut self,
        map: Root<'s, Map>,
        key: Root<'s, Str>,
        value: Option<Root<'s, M>>,
    ) {
        let (map, key) = (map.raw(self), key.raw(self));
        let value = value.map_or(0, |value| value.raw(self));
        // SAFETY: a live map of string keys whose values are references,
        // a string and an object or zero.
        unsafe {
            crate::map::set(
                self.mutator(),
                map,
                crate::map::KeyKind::Str,
                true,
                key,
                value,
            );
        }
    }

    /// The error object at `error`, to throw.
    ///
    /// # Safety
    ///
    /// `error` is a live object.
    unsafe fn thrown(&mut self, error: usize) -> Thrown<'s> {
        Thrown {
            // SAFETY: the caller's contract.
            error: unsafe { self.elem_raw(error, TypeId(0)) },
            context: None,
        }
    }

    /// The type id of the storage behind arrays of type `array_type`.
    pub(crate) fn storage_type(&self, array_type: usize) -> usize {
        // SAFETY: as in `Self::mutator`.
        unsafe { self.mutator.as_ref() }.storage_type(array_type)
    }
}

/// What the body of [`Scope::scoped`] is handed beside its scope: that the
/// outer scope, of `'s`, outlives its own, of `'n`.
pub struct Within<'n, 's: 'n>(PhantomData<(&'n (), &'s ())>);

impl Drop for Scope<'_> {
    fn drop(&mut self) {
        if self.exchange.is_null() {
            let mut rooted = 0;
            self.slots.each_root(|_| rooted += 1);
            self.mutator().unroot(rooted);
        }
    }
}

/// The bytes a string of `bytes` bytes of text takes.
pub const fn str_size(bytes: usize) -> usize {
    STRING_TEXT + words(bytes)
}

/// The bytes [`Reservation::part`] takes for a part of `bytes` bytes of
/// text: a copy is never larger than a slice, so it is the smaller of the
/// two, with no test of which the part will be.
pub const fn part_size(bytes: usize) -> usize {
    let copied = str_size(bytes);
    if copied < SLICE_SIZE {
        copied
    } else {
        SLICE_SIZE
    }
}

/// The bytes an array of `length` elements takes, storage included.
pub const fn array_size(length: usize) -> usize {
    if length == 0 {
        ARRAY_SIZE
    } else {
        ARRAY_SIZE + STORAGE_ELEMENTS + length * WORD
    }
}

/// Room for several objects, in which nothing moves.
pub struct Reservation<'r, 's> {
    scope: &'r mut Scope<'s>,
    next: Cell<usize>,
    end: usize,
}

impl<'s> Reservation<'_, 's> {
    fn take(&self, bytes: usize) -> usize {
        let start = self.next.get();
        assert!(
            start + bytes <= self.end,
            "a native builds more than it reserved"
        );
        self.next.set(start + bytes);
        start
    }

    /// The object behind `root`, good until the reservation is finished.
    pub fn get<M: Managed>(&self, root: Root<'s, M>) -> M::View<'_> {
        // SAFETY: nothing moves while the reservation is open.
        unsafe { M::View::from_raw(root.raw(self.scope)) }
    }

    /// A new string holding `text`.
    pub fn str(&self, text: &str) -> StrRef<'_> {
        self.counted_str(text, text.chars().count())
    }

    /// A new string holding `text`, which has `chars` characters.
    pub fn counted_str(&self, text: &str, chars: usize) -> StrRef<'_> {
        assert!(
            u32::try_from(text.len()).is_ok(),
            "a string cannot be longer than 4 GiB"
        );
        debug_assert_eq!(text.chars().count(), chars);
        let size = str_size(text.len());
        let string = self.take(size);
        // SAFETY: the string has room for its two words and the text. The
        // last word first, for the zeros after the text.
        unsafe {
            store(string + size - WORD, 0);
            store(string, TYPE_STRING);
            store(
                string + STRING_LENGTHS,
                text.len() | (chars << STRING_CHAR_COUNT_SHIFT),
            );
            std::ptr::copy_nonoverlapping(
                text.as_ptr(),
                (string + STRING_TEXT) as *mut u8,
                text.len(),
            );
            StrRef::from_raw(string)
        }
    }

    /// Bytes `from..from + bytes` of `whole`, which are `chars` whole
    /// characters, as a string: a copy when short, and otherwise a slice of
    /// the string that holds the text.
    ///
    /// # Safety
    ///
    /// The range is inside the text and starts and ends between
    /// characters, of which it has `chars`.
    #[inline]
    pub unsafe fn part(
        &self,
        whole: Whole<'_>,
        from: usize,
        bytes: usize,
        chars: usize,
    ) -> StrRef<'_> {
        if bytes < SLICE_MIN_BYTES {
            // SAFETY: the caller's contract.
            let text = unsafe { whole.text.get_unchecked(from..from + bytes) };
            return self.counted_str(text, chars);
        }
        debug_assert!(whole.text.is_char_boundary(from));
        debug_assert_eq!(whole.text[from..from + bytes].chars().count(), chars);
        let slice = self.take(SLICE_SIZE);
        // SAFETY: the slice has room for its four words.
        unsafe {
            store(slice, TYPE_STRING_SLICE);
            store(
                slice + STRING_LENGTHS,
                bytes | (chars << STRING_CHAR_COUNT_SHIFT),
            );
            store(slice + SLICE_PARENT, whole.holder);
            store(slice + SLICE_OFFSET, whole.start + from);
            StrRef::from_raw(slice)
        }
    }

    /// A new string of `bytes` bytes of text and `chars` characters, its
    /// text to be written front to back.
    pub fn new_str(&self, bytes: usize, chars: usize) -> NewStr<'_> {
        assert!(
            u32::try_from(bytes).is_ok(),
            "a string cannot be longer than 4 GiB"
        );
        let size = str_size(bytes);
        let string = self.take(size);
        // SAFETY: the string has room for its two words. The last word
        // first, for the zeros after the text.
        unsafe {
            store(string + size - WORD, 0);
            store(string, TYPE_STRING);
            store(
                string + STRING_LENGTHS,
                bytes | (chars << STRING_CHAR_COUNT_SHIFT),
            );
        }
        NewStr {
            string,
            written: 0,
            bytes,
            reservation: PhantomData,
        }
    }

    /// A new array of `length` elements, each zero.
    pub fn array<E: Element>(&self, length: usize) -> NewArray<'_, E> {
        self.array_of(length, E::IS_REF)
    }

    /// A new array of `length` values of a type parameter, each zero.
    pub fn slots(&self, length: usize, holds_refs: bool) -> NewArray<'_, Slot> {
        self.array_of(length, holds_refs)
    }

    fn array_of<E: Element>(&self, length: usize, holds_refs: bool) -> NewArray<'_, E> {
        let array = self.take(array_size(length));
        let array_type = if holds_refs {
            TYPE_ARRAY_OF_REFS
        } else {
            TYPE_ARRAY_OF_VALUES
        };
        // SAFETY: the array has room for itself and its storage.
        unsafe {
            store(array, array_type);
            store(array + ARRAY_LENGTH, length);
            if length == 0 {
                store(array + ARRAY_DATA, 0);
            } else {
                let storage = array + ARRAY_SIZE;
                let data = storage + STORAGE_ELEMENTS;
                store(storage, self.scope.storage_type(array_type));
                store(storage + STORAGE_CAPACITY, length);
                std::ptr::write_bytes(data as *mut usize, 0, length);
                store(array + ARRAY_DATA, data);
            }
        }
        NewArray(array, PhantomData)
    }

    /// Close the reservation, keeping an object built in it.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "the object is handed over: it is good for one reservation"
    )]
    pub fn finish<M: Managed>(self, object: Built<M>) -> Root<'s, M> {
        // SAFETY: built in this reservation, as an `M`.
        unsafe { self.scope.root_raw(object.0) }
    }
}

/// An object built in a reservation, on its way to being rooted when the
/// reservation is finished.
pub struct Built<M>(usize, PhantomData<M>);

/// A string being written, inside a reservation: its length and character
/// count are set, and its text is appended piece by piece.
pub struct NewStr<'a> {
    string: usize,
    written: usize,
    bytes: usize,
    reservation: PhantomData<&'a ()>,
}

impl<'a> NewStr<'a> {
    /// Append `text`.
    pub fn push(&mut self, text: &str) {
        assert!(
            text.len() <= self.bytes - self.written,
            "past the string's end"
        );
        // SAFETY: the string has room for `bytes` bytes of text, checked
        // here.
        unsafe {
            std::ptr::copy_nonoverlapping(
                text.as_ptr(),
                (self.string + STRING_TEXT + self.written) as *mut u8,
                text.len(),
            );
        }
        self.written += text.len();
    }

    /// Append one character.
    pub fn push_char(&mut self, character: char) {
        self.push(character.encode_utf8(&mut [0; 4]));
    }

    /// What has been written so far.
    fn text(&mut self) -> &mut [u8] {
        // SAFETY: that many bytes have been written, and nothing else
        // refers to a string still being built.
        unsafe {
            std::slice::from_raw_parts_mut((self.string + STRING_TEXT) as *mut u8, self.written)
        }
    }

    /// Lowercase the ASCII letters written so far, in place.
    pub fn make_ascii_lowercase(&mut self) {
        self.text().make_ascii_lowercase();
    }

    /// Uppercase the ASCII letters written so far, in place.
    pub fn make_ascii_uppercase(&mut self) {
        self.text().make_ascii_uppercase();
    }

    /// The string, which must have been written in full.
    pub fn finish(self) -> StrRef<'a> {
        assert!(
            self.written == self.bytes,
            "a string is not written in full"
        );
        // SAFETY: a string in a reservation does not move until the
        // reservation is finished.
        let string = unsafe { StrRef::from_raw(self.string) };
        debug_assert_eq!(string.as_str().chars().count(), string.char_count());
        string
    }
}

/// An array being filled in, inside a reservation.
pub struct NewArray<'a, E>(usize, PhantomData<(&'a (), E)>);

impl<E> Clone for NewArray<'_, E> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<E> Copy for NewArray<'_, E> {}

impl<E: Element> NewArray<'_, E> {
    /// The array, to finish its reservation with.
    pub fn keep(self) -> Built<Arr<E>> {
        Built(self.0, PhantomData)
    }

    fn put(self, index: usize, word: usize) {
        // SAFETY: a new array has storage for its length, checked here.
        unsafe {
            assert!(index < load(self.0 + ARRAY_LENGTH), "past the array's end");
            store(load(self.0 + ARRAY_DATA) + index * WORD, word);
        }
    }
}

impl<'a, M: Managed> NewArray<'a, M> {
    pub fn set(self, index: usize, value: M::View<'a>) {
        self.put(index, value.raw());
    }
}

impl NewArray<'_, Slot> {
    /// Store the word of a value of the array's element type.
    pub fn set(self, index: usize, word: usize) {
        self.put(index, word);
    }
}

impl NewArray<'_, Int> {
    pub fn set(self, index: usize, value: i64) {
        self.put(index, value.cast_unsigned() as usize);
    }
}

impl NewArray<'_, Float> {
    pub fn set(self, index: usize, value: f64) {
        self.put(index, value.to_bits() as usize);
    }
}

impl NewArray<'_, Bool> {
    pub fn set(self, index: usize, value: bool) {
        self.put(index, usize::from(value));
    }
}

/// The standard library natives: each class's generated trait is
/// implemented for this type, in the module named after the class.
pub struct Natives;

mod array;
mod bigint;
pub(crate) mod build;
mod crypto;
pub(crate) mod drive;
mod env;
mod float;
mod fs;
mod hash;
#[cfg(feature = "http")]
mod http;
mod int;
mod json;
mod map;
mod regex;
mod root;
mod state;
mod string;
mod uint8array;

pub use build::{Build, Class, Value};
pub use state::{Opaque, OpaqueRef};

#[allow(
    clippy::all,
    clippy::pedantic,
    non_snake_case,
    async_fn_in_trait,
    reason = "generated from the standard library's declarations"
)]
mod generated {
    use super::drive::{Head, start};
    #[allow(unused_imports, reason = "what the listed natives need varies")]
    use super::{
        Arr, ArrRef, Big, BigRef, Bool, Bytes, BytesRef, Elem, Float, Int, Map, MapRef, Natives,
        Obj, ObjRef, Opaque, OpaqueRef, OptionalScalar, Root, RootSlots, RtResult, Scope, Slot,
        Str, StrRef, Thrown, TypeId, View,
    };
    use crate::{heap::Mutator, stubs::anchored};

    include!(concat!(env!("OUT_DIR"), "/natives_generated.rs"));
}
pub use generated::*;
