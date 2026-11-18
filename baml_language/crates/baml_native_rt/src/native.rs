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

#![allow(
    clippy::cast_possible_truncation,
    reason = "a word is 64 bits wherever the runtime is built"
)]

use std::{cell::Cell, marker::PhantomData};

use crate::{
    heap::{Mutator, load, store},
    layout::{
        ARRAY_DATA, ARRAY_LENGTH, ARRAY_SIZE, STORAGE_CAPACITY, STORAGE_ELEMENTS,
        STRING_CHAR_COUNT_SHIFT, STRING_LENGTHS, STRING_TEXT, TYPE_ARRAY_OF_REFS,
        TYPE_ARRAY_OF_VALUES, TYPE_ID_MASK, TYPE_STRING, WORD, field_offset, instance_size, words,
    },
};

/// How many roots one call of a native can hold at once.
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
/// An array of `E`.
pub struct Arr<E>(PhantomData<E>);
/// A class instance.
pub struct Obj;
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
impl<E: Element> Managed for Arr<E> {
    type View<'a> = ArrRef<'a, E>;
}
impl Managed for Obj {
    type View<'a> = ObjRef<'a>;
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
        self.char_count() == self.as_str().len()
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

/// The slots behind the roots of one call of a native. It lives in the
/// frame of the function compiled code called.
pub struct RootSlots([Cell<usize>; ROOTS]);

impl RootSlots {
    pub fn new() -> Self {
        Self([const { Cell::new(0) }; ROOTS])
    }
}

impl Default for RootSlots {
    fn default() -> Self {
        Self::new()
    }
}

/// A reference the collector keeps current, for as long as the call it was
/// made in.
pub struct Root<'s, M>(&'s Cell<usize>, PhantomData<M>);

impl<M> Clone for Root<'_, M> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<M> Copy for Root<'_, M> {}

impl<M: Managed> Root<'_, M> {
    /// The object, for as long as `scope` allocates nothing.
    pub fn get<'a>(self, _scope: &'a Scope<'_>) -> M::View<'a> {
        // SAFETY: the slot holds a live `M`, which moves only when the
        // scope is borrowed exclusively.
        unsafe { M::View::from_raw(self.0.get()) }
    }

    /// The address to hand back to compiled code.
    pub fn raw(self) -> usize {
        self.0.get()
    }
}

/// The error a native throws: the object compiled code receives. Until the
/// type table names the standard library's error classes, that object is
/// the text an uncaught throw of the error prints.
pub struct Thrown<'s>(Root<'s, Str>);

/// A value of a type parameter held across allocations: a root when the
/// type's values are references, the plain word otherwise.
#[derive(Clone, Copy)]
pub struct Elem<'s> {
    slot: &'s Cell<usize>,
    is_ref: bool,
}

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
            thrown: thrown.0.raw(),
        }
    }
}

/// One call of a native: the mutator it allocates with and the roots it
/// holds.
pub struct Scope<'s> {
    mutator: &'s mut Mutator,
    slots: &'s RootSlots,
    used: usize,
}

impl<'s> Scope<'s> {
    /// # Safety
    ///
    /// The mutator has been told the frame compiled code called
    /// ([`Mutator::enter`]).
    pub unsafe fn new(mutator: &'s mut Mutator, slots: &'s RootSlots) -> Self {
        Self {
            mutator,
            slots,
            used: 0,
        }
    }

    /// Root the object at `raw`.
    ///
    /// # Safety
    ///
    /// `raw` is a live `M`.
    pub unsafe fn root_raw<M: Managed>(&mut self, raw: usize) -> Root<'s, M> {
        assert!(self.used < ROOTS, "a native holds at most {ROOTS} roots");
        let slot = &self.slots.0[self.used];
        slot.set(raw);
        self.mutator.root_at(slot.as_ptr() as usize);
        self.used += 1;
        Root(slot, PhantomData)
    }

    /// Hold `word`, a value of the type a call names by `ty`.
    ///
    /// # Safety
    ///
    /// `word` is a value of that type: a live object or zero when the
    /// type's values are references.
    pub unsafe fn elem_raw(&mut self, word: usize, ty: TypeId) -> Elem<'s> {
        assert!(self.used < ROOTS, "a native holds at most {ROOTS} roots");
        let slot = &self.slots.0[self.used];
        slot.set(word);
        let is_ref = self.is_reference(ty);
        if is_ref {
            self.mutator.root_at(slot.as_ptr() as usize);
            self.used += 1;
        }
        Elem { slot, is_ref }
    }

    /// Whether values of the type a call names by `ty` are references.
    pub fn is_reference(&self, ty: TypeId) -> bool {
        self.mutator.is_reference(ty.0 as usize)
    }

    /// A new string holding `text`. May collect.
    pub fn alloc_str(&mut self, text: &str) -> Root<'s, Str> {
        let string = crate::string::from_str(self.mutator, text);
        // SAFETY: just made.
        unsafe { self.root_raw(string) }
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
        let object = self.mutator.allocate_object(type_id, size);
        // SAFETY: the object has `slots` words after its header.
        unsafe {
            std::ptr::write_bytes((object + field_offset(0)) as *mut usize, 0, slots);
            self.root_raw(object)
        }
    }

    /// Room for objects of `bytes` bytes in all ([`str_size`],
    /// [`array_size`]). May collect, once, now.
    pub fn reserve(&mut self, bytes: usize) -> Reservation<'_, 's> {
        let start = self.mutator.allocate(bytes);
        Reservation {
            scope: self,
            next: Cell::new(start),
            end: start + bytes,
        }
    }

    /// Throw `baml.errors.InvalidArgument`.
    pub fn invalid_argument(&mut self, message: &str) -> Thrown<'s> {
        Thrown(self.alloc_str(&format!(
            "baml.errors.InvalidArgument {{message: {message:?}}}"
        )))
    }

    /// Throw `baml.errors.ParseError`.
    pub fn parse_error(&mut self, message: &str) -> Thrown<'s> {
        Thrown(self.alloc_str(&format!("baml.errors.ParseError {{message: {message:?}}}")))
    }
}

impl Drop for Scope<'_> {
    fn drop(&mut self) {
        self.mutator.unroot(self.used);
    }
}

/// The bytes a string of `bytes` bytes of text takes.
pub const fn str_size(bytes: usize) -> usize {
    STRING_TEXT + words(bytes)
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
        unsafe { M::View::from_raw(root.raw()) }
    }

    /// A new string holding `text`.
    pub fn str(&self, text: &str) -> StrRef<'_> {
        assert!(
            u32::try_from(text.len()).is_ok(),
            "a string cannot be longer than 4 GiB"
        );
        let string = self.take(str_size(text.len()));
        // SAFETY: the string has room for its two words and the text.
        unsafe {
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
            StrRef::from_raw(string)
        }
    }

    /// A new string of `bytes` bytes of text and `chars` characters, its
    /// text to be written front to back.
    pub fn new_str(&self, bytes: usize, chars: usize) -> NewStr<'_> {
        assert!(
            u32::try_from(bytes).is_ok(),
            "a string cannot be longer than 4 GiB"
        );
        let string = self.take(str_size(bytes));
        // SAFETY: the string has room for its two words.
        unsafe {
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
                store(storage, self.scope.mutator.storage_type(array_type));
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
mod float;
mod int;
mod root;
mod string;

#[allow(
    clippy::all,
    clippy::pedantic,
    non_snake_case,
    reason = "generated from the standard library's declarations"
)]
mod generated {
    #[allow(unused_imports, reason = "what the listed natives need varies")]
    use super::{
        Arr, ArrRef, Bool, Elem, Float, Int, Natives, OptionalScalar, Root, RootSlots, RtResult,
        Scope, Slot, Str, StrRef, Thrown, TypeId, View,
    };
    use crate::heap::{Mutator, current_frame};

    include!(concat!(env!("OUT_DIR"), "/natives_generated.rs"));
}
pub use generated::*;
