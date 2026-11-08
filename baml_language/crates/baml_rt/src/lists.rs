//! Typed growing lists. Known-capacity lists embed their initial buffer in the
//! stable object; growth replaces the buffer pointer without invalidating aliases.
mod natural_sort;
use crate::{
    R,
    language::{allocation_failure, index_error},
    strings::{Cx, Str},
};
use baml_rt_gc::{Descriptor, ElementLayout, Elements, Header, Kind, write_ptr};
use std::{marker::PhantomData, rc::Rc};

/// A typed list element. Implementations define their exact GC layout and stores.
///
/// # Safety
/// `SIZE` must match `LAYOUT`; reads and writes must access exactly one slot,
/// and pointer-bearing writes must use the collector's store hooks.
pub unsafe trait Slot: Copy {
    const SIZE: usize;
    const LAYOUT: ElementLayout;
    /// # Safety
    /// `slot` points to an initialized slot of this type in a live buffer.
    unsafe fn read(slot: *const u8) -> Self;
    /// # Safety
    /// `slot` is a slot in `object`, and all pointer payloads are live.
    unsafe fn write(self, object: *mut u8, slot: *mut u8);
}
macro_rules! scalar_slot {
    ($ty:ty, $read:expr, $write:expr) => {
        unsafe impl Slot for $ty {
            const SIZE: usize = 8;
            const LAYOUT: ElementLayout = ElementLayout::Scalar;
            #[inline(always)]
            unsafe fn read(slot: *const u8) -> Self {
                ($read)(unsafe { slot.cast::<u64>().read() })
            }
            #[inline(always)]
            unsafe fn write(self, _object: *mut u8, slot: *mut u8) {
                unsafe { slot.cast::<u64>().write(($write)(self)) }
            }
        }
    };
}
scalar_slot!(i64, |v| v as i64, |v: i64| v as u64);
scalar_slot!(f64, f64::from_bits, f64::to_bits);
scalar_slot!(bool, |v| v != 0, |v: bool| v as u64);
/// An opaque heap pointer slot, also used for classes, maps and nested lists.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct Ptr(usize, PhantomData<Rc<()>>);
impl Ptr {
    /// # Safety
    /// `pointer` is null or names a live object in the active instance.
    pub unsafe fn from_raw(pointer: usize) -> Self {
        Self(pointer, PhantomData)
    }
    pub fn as_raw(self) -> usize {
        self.0
    }
}
macro_rules! pointer_slot {
    ($ty:ty) => {
        unsafe impl Slot for $ty {
            const SIZE: usize = 8;
            const LAYOUT: ElementLayout = ElementLayout::Pointer;
            #[inline(always)]
            unsafe fn read(slot: *const u8) -> Self {
                unsafe { Self::from_raw(slot.cast::<usize>().read()) }
            }
            #[inline(always)]
            unsafe fn write(self, object: *mut u8, slot: *mut u8) {
                unsafe { write_ptr(object, slot.cast(), self.as_raw()) }
            }
        }
    };
}
pointer_slot!(Ptr);
pointer_slot!(Str);
unsafe impl Slot for crate::Dyn {
    const SIZE: usize = 16;
    const LAYOUT: ElementLayout = ElementLayout::Tagged;
    #[inline(always)]
    unsafe fn read(slot: *const u8) -> Self {
        unsafe { slot.cast::<Self>().read() }
    }
    #[inline(always)]
    unsafe fn write(self, object: *mut u8, slot: *mut u8) {
        unsafe { crate::write_dyn(object, slot.cast(), self) }
    }
}
#[repr(C)]
struct Object {
    header: Header,
    buffer: usize,
    len: usize,
    capacity: usize,
}

#[repr(transparent)]
pub struct List<T: Slot> {
    pointer: usize,
    owner: PhantomData<(T, Rc<()>)>,
}
impl<T: Slot> Copy for List<T> {}
impl<T: Slot> Clone for List<T> {
    fn clone(&self) -> Self {
        *self
    }
}
unsafe impl<T: Slot> Slot for List<T> {
    const SIZE: usize = 8;
    const LAYOUT: ElementLayout = ElementLayout::Pointer;
    #[inline(always)]
    unsafe fn read(slot: *const u8) -> Self {
        unsafe { Self::from_raw(slot.cast::<usize>().read()) }
    }
    #[inline(always)]
    unsafe fn write(self, object: *mut u8, slot: *mut u8) {
        unsafe { write_ptr(object, slot.cast(), self.as_raw()) }
    }
}

/// The compiler emits this descriptor and then the corresponding buffer descriptor.
pub const fn descriptor<T: Slot>(type_id: u32) -> Descriptor {
    Descriptor {
        kind: Kind::List,
        size: T::SIZE,
        ptr_slots: (8, 1),
        tagged_slots: (0, 0),
        elem: Some(Elements {
            offset: 32,
            layout: T::LAYOUT,
        }),
        type_id,
        fields: &[],
        finalizer: None,
    }
}
pub const fn buffer_descriptor<T: Slot>(type_id: u32) -> Descriptor {
    Descriptor {
        kind: Kind::List,
        size: T::SIZE,
        ptr_slots: (0, 0),
        tagged_slots: (0, 0),
        elem: Some(Elements {
            offset: 16,
            layout: T::LAYOUT,
        }),
        type_id,
        fields: &[],
        finalizer: None,
    }
}
impl<T: Slot> List<T> {
    /// # Safety
    /// `desc` and `desc+1` describe this list and its T buffer as above. All
    /// handles obey Cx's root invocation and rooting contract.
    #[inline(always)]
    pub unsafe fn new(_cx: &mut Cx, desc: u32) -> R<Self> {
        let pointer = unsafe { crate::try_alloc(desc, 32) }? as usize;
        unsafe {
            let object = pointer as *mut Object;
            write_ptr(object.cast(), &raw mut (*object).buffer, 0);
            (*object).len = 0;
            (*object).capacity = 0;
            Ok(Self::from_raw(pointer))
        }
    }
    /// # Safety
    /// `pointer` names a live list of T with the descriptor pair above. The
    /// handle and copies must obey the active Cx's lifetime/rooting contract.
    pub unsafe fn from_raw(pointer: usize) -> Self {
        Self {
            pointer,
            owner: PhantomData,
        }
    }
    pub fn as_raw(self) -> usize {
        self.pointer
    }
    #[inline(always)]
    pub fn len(self, _cx: &Cx) -> i64 {
        unsafe { (*(self.pointer as *const Object)).len as i64 }
    }
    pub fn is_empty(self, cx: &Cx) -> bool {
        self.len(cx) == 0
    }
    #[inline(always)]
    pub fn get(self, cx: &Cx, index: i64) -> R<T> {
        let len = self.len(cx);
        let resolved = if index < 0 { index + len } else { index };
        if resolved < 0 || resolved >= len {
            return Err(index_error(index, len as usize));
        }
        unsafe {
            let buffer = (*(self.pointer as *const Object)).buffer;
            Ok(T::read(
                ((buffer + 16) as *const u8).add(resolved as usize * T::SIZE),
            ))
        }
    }
    /// Independent snapshot; immutable elements may be shared.
    pub fn copy(self, cx: &mut Cx) -> R<Self> {
        let desc = unsafe { (*(self.pointer as *const Object)).header.desc };
        let len = self.len(cx) as usize;
        let result = unsafe { Self::with_capacity(cx, desc, len) }?;
        for i in 0..len {
            result.push(cx, self.get(cx, i as i64)?)?;
        }
        Ok(result)
    }
    #[inline(always)]
    pub fn set(self, cx: &Cx, index: i64, value: T) -> R<()> {
        let len = self.len(cx);
        let resolved = if index < 0 { index + len } else { index };
        if resolved < 0 || resolved >= len {
            return Err(index_error(index, len as usize));
        }
        unsafe {
            let buffer = (*(self.pointer as *const Object)).buffer as *mut u8;
            value.write(
                self.buffer_owner(buffer),
                buffer.add(16 + resolved as usize * T::SIZE),
            );
        }
        Ok(())
    }
    #[inline(always)]
    pub fn push(self, cx: &mut Cx, value: T) -> R<i64> {
        let object = self.pointer as *mut Object;
        let len = unsafe { (*object).len };
        if len == unsafe { (*object).capacity } {
            self.grow(cx, len + 1)?;
        }
        unsafe {
            let buffer = (*object).buffer as *mut u8;
            value.write(self.buffer_owner(buffer), buffer.add(16 + len * T::SIZE));
            (*object).len = len + 1;
        }
        Ok((len + 1) as i64)
    }
    #[cold]
    #[inline(never)]
    fn grow(self, _cx: &mut Cx, required: usize) -> R<()> {
        let object = self.pointer as *mut Object;
        let (len, capacity, desc) =
            unsafe { ((*object).len, (*object).capacity, (*object).header.desc) };
        let capacity = capacity
            .checked_mul(2)
            .map(|n| n.max(4).max(required))
            .filter(|n| *n <= baml_type::Int63::MAX.get() as usize)
            .ok_or_else(|| allocation_failure(usize::MAX))?;
        let bytes = capacity
            .checked_mul(T::SIZE)
            .and_then(|n| n.checked_add(16))
            .ok_or_else(|| allocation_failure(usize::MAX))?;
        let buffer = unsafe { crate::try_alloc(desc + 1, bytes) }?;
        unsafe {
            let old = (*object).buffer as *const u8;
            for i in 0..len {
                let value = T::read(old.add(16 + i * T::SIZE));
                value.write(buffer, buffer.add(16 + i * T::SIZE));
            }
            if old as usize == self.pointer + 16 {
                for i in 0..len {
                    self.clear_slot(old as *mut u8, i);
                }
            }
            write_ptr(object.cast(), &raw mut (*object).buffer, buffer as usize);
            (*object).capacity = capacity;
        }
        Ok(())
    }
    /// # Safety
    /// The descriptor pair and handles follow `new`'s contract.
    #[inline(always)]
    pub unsafe fn with_capacity(cx: &mut Cx, desc: u32, capacity: usize) -> R<Self> {
        if capacity == 0 {
            return unsafe { Self::new(cx, desc) };
        }
        let capacity = capacity.max(4);
        let bytes = capacity
            .checked_mul(T::SIZE)
            .and_then(|n| n.checked_add(32))
            .filter(|_| capacity <= baml_type::Int63::MAX.get() as usize)
            .ok_or_else(|| allocation_failure(usize::MAX))?;
        let pointer = unsafe { crate::try_alloc(desc, bytes) }? as usize;
        unsafe {
            let object = pointer as *mut Object;
            // The buffer ABI addresses elements at buffer+16. This interior
            // pointer resolves to this object in the conservative root scanner.
            write_ptr(object.cast(), &raw mut (*object).buffer, pointer + 16);
            (*object).len = 0;
            (*object).capacity = capacity;
            Ok(Self::from_raw(pointer))
        }
    }
    /// # Safety
    /// The descriptor pair describes List<T> and its buffer.
    pub unsafe fn generate(
        cx: &mut Cx,
        desc: u32,
        length: i64,
        mut f: impl FnMut(i64) -> R<T>,
    ) -> R<Self> {
        let list = unsafe { Self::new(cx, desc) }?;
        for i in 0..length.max(0) {
            let value = f(i)?;
            list.push(cx, value)?;
            if i & 15 == 15 {
                cx.poll()?;
            }
        }
        Ok(list)
    }
    pub fn at(self, cx: &Cx, index: i64) -> Option<T> {
        let len = self.len(cx);
        let index = if index < 0 { index + len } else { index };
        if index < 0 || index >= len {
            return None;
        }
        unsafe {
            let buffer = (*(self.pointer as *const Object)).buffer as *const u8;
            Some(T::read(buffer.add(16 + index as usize * T::SIZE)))
        }
    }
    #[inline(always)]
    fn buffer_owner(self, buffer: *mut u8) -> *mut u8 {
        if buffer as usize == self.pointer + 16 {
            self.pointer as *mut u8
        } else {
            buffer
        }
    }
    unsafe fn clear_slot(self, buffer: *mut u8, index: usize) {
        let slot = unsafe { buffer.add(16 + index * T::SIZE) };
        unsafe {
            match T::LAYOUT {
                ElementLayout::Pointer => write_ptr(self.buffer_owner(buffer), slot.cast(), 0),
                ElementLayout::Tagged => crate::write_dyn(
                    self.buffer_owner(buffer),
                    slot.cast(),
                    crate::Dyn { tag: 2, payload: 0 },
                ),
                ElementLayout::Scalar => slot.cast::<u64>().write(0),
                ElementLayout::Descriptor(_) => {
                    unreachable!("Slot does not use inline descriptors")
                }
            }
        }
    }
    pub fn clear(self, cx: &Cx) {
        unsafe {
            let object = self.pointer as *mut Object;
            let buffer = (*object).buffer as *mut u8;
            for i in 0..self.len(cx) as usize {
                self.clear_slot(buffer, i);
            }
            (*object).len = 0;
        }
    }
    pub fn pop(self, cx: &Cx) -> Option<T> {
        self.remove_at(cx, -1)
    }
    pub fn remove_at(self, cx: &Cx, index: i64) -> Option<T> {
        let value = self.at(cx, index)?;
        let len = self.len(cx);
        let index = if index < 0 { index + len } else { index } as usize;
        unsafe {
            let object = self.pointer as *mut Object;
            let buffer = (*object).buffer as *mut u8;
            for i in index..len as usize - 1 {
                T::read(buffer.add(16 + (i + 1) * T::SIZE))
                    .write(self.buffer_owner(buffer), buffer.add(16 + i * T::SIZE));
            }
            self.clear_slot(buffer, len as usize - 1);
            (*object).len -= 1;
        }
        Some(value)
    }
    pub fn unshift(self, cx: &mut Cx, value: T) -> R<i64> {
        let len = self.len(cx) as usize;
        self.push(cx, value)?;
        unsafe {
            let buffer = (*(self.pointer as *const Object)).buffer as *mut u8;
            for i in (0..len).rev() {
                T::read(buffer.add(16 + i * T::SIZE)).write(
                    self.buffer_owner(buffer),
                    buffer.add(16 + (i + 1) * T::SIZE),
                );
            }
            value.write(self.buffer_owner(buffer), buffer.add(16));
        }
        Ok(len as i64 + 1)
    }
    pub fn insert(self, cx: &mut Cx, value: T, index: i64) -> R<i64> {
        let len = self.len(cx);
        let resolved = if index < 0 { index + len } else { index };
        if resolved < 0 || resolved > len {
            return Err(crate::list_errors::invalid_argument(
                cx,
                format!("array.insert: idx ({index}) is out of range for length {len}"),
            ));
        }
        self.push(cx, value)?;
        unsafe {
            let buffer = (*(self.pointer as *const Object)).buffer as *mut u8;
            for i in (resolved as usize..len as usize).rev() {
                T::read(buffer.add(16 + i * T::SIZE)).write(
                    self.buffer_owner(buffer),
                    buffer.add(16 + (i + 1) * T::SIZE),
                );
            }
            value.write(
                self.buffer_owner(buffer),
                buffer.add(16 + resolved as usize * T::SIZE),
            );
        }
        Ok(len + 1)
    }
    // An embedded buffer cannot be detached from its original object. Copy
    // transactionally instead, preserving the receiver identity and aliases.
    fn replace_contents(self, cx: &mut Cx, source: Self) -> R<()> {
        if self.pointer == source.pointer {
            return Ok(());
        }
        let len = source.len(cx) as usize;
        let object = self.pointer as *mut Object;
        if len > unsafe { (*object).capacity } {
            self.grow(cx, len)?;
        }
        unsafe {
            let buffer = (*object).buffer as *mut u8;
            for i in 0..len {
                source
                    .get(cx, i as i64)?
                    .write(self.buffer_owner(buffer), buffer.add(16 + i * T::SIZE));
            }
            for i in len..(*object).len {
                self.clear_slot(buffer, i);
            }
            (*object).len = len;
        }
        Ok(())
    }
    pub fn splice(self, cx: &mut Cx, start: i64, count: i64, replace: Self) -> R<()> {
        let len = self.len(cx);
        let resolved = if start < 0 { start + len } else { start };
        if resolved < 0 || resolved > len {
            return Err(crate::list_errors::invalid_argument(
                cx,
                format!("array.splice: start ({start}) is out of range for length {len}"),
            ));
        }
        if count < 0 {
            return Err(crate::list_errors::invalid_argument(
                cx,
                format!("array.splice: count ({count}) is negative"),
            ));
        }
        let end = resolved.saturating_add(count).min(len);
        let replacement_len = replace.len(cx);
        let desc = unsafe { (*(self.pointer as *const Object)).header.desc };
        let result = unsafe {
            Self::with_capacity(
                cx,
                desc,
                (len - (end - resolved) + replacement_len) as usize,
            )
        }?;
        for (list, start, end) in [
            (self, 0, resolved),
            (replace, 0, replacement_len),
            (self, end, len),
        ] {
            for i in start..end {
                let value = list.get(cx, i)?;
                result.push(cx, value)?;
            }
        }
        self.replace_contents(cx, result)?;
        Ok(())
    }
    /// # Safety
    /// The descriptor pair describes List<T> and its buffer.
    pub unsafe fn filled(cx: &mut Cx, desc: u32, length: i64, value: T) -> R<Self> {
        let length = length.max(0) as usize;
        let result = unsafe { Self::with_capacity(cx, desc, length) }?;
        for _ in 0..length {
            result.push(cx, value)?;
        }
        Ok(result)
    }
    pub fn slice(self, cx: &mut Cx, start: i64, end: i64) -> R<Self> {
        let len = self.len(cx);
        let bound = |n: i64| (if n < 0 { n + len } else { n }).clamp(0, len);
        let start = bound(start);
        let end = bound(end).max(start);
        let desc = unsafe { (*(self.pointer as *const Object)).header.desc };
        let result = unsafe { Self::with_capacity(cx, desc, (end - start) as usize) }?;
        for i in start..end {
            let value = self.get(cx, i)?;
            result.push(cx, value)?;
        }
        Ok(result)
    }
    pub fn reverse(self, cx: &mut Cx) -> R<Self> {
        let len = self.len(cx);
        let desc = unsafe { (*(self.pointer as *const Object)).header.desc };
        let result = unsafe { Self::with_capacity(cx, desc, len as usize) }?;
        for i in (0..len).rev() {
            let value = self.get(cx, i)?;
            result.push(cx, value)?;
        }
        Ok(result)
    }
    pub fn concat(self, cx: &mut Cx, other: Self) -> R<Self> {
        let len = self.len(cx);
        let other_len = other.len(cx);
        let desc = unsafe { (*(self.pointer as *const Object)).header.desc };
        let result = unsafe { Self::with_capacity(cx, desc, (len + other_len) as usize) }?;
        for (list, n) in [(self, len), (other, other_len)] {
            for i in 0..n {
                let value = list.get(cx, i)?;
                result.push(cx, value)?;
            }
        }
        Ok(result)
    }

    pub fn index_of(
        self,
        cx: &Cx,
        item: T,
        last: bool,
        mut equal: impl FnMut(T, T) -> bool,
    ) -> Option<i64> {
        let len = self.len(cx);
        for offset in 0..len {
            let index = if last { len - offset - 1 } else { offset };
            let value = self.at(cx, index).expect("in-bounds list search");
            if equal(value, item) {
                return Some(index);
            }
        }
        None
    }
    /// Callback walks use a traced snapshot: callbacks may allocate, collect,
    /// suspend, or mutate the receiver without invalidating the walk.
    pub fn for_each(self, cx: &mut Cx, mut f: impl FnMut(T) -> R<()>) -> R<()> {
        let snapshot = self.slice(cx, 0, self.len(cx))?;
        for i in 0..snapshot.len(cx) {
            f(snapshot.get(cx, i)?)?;
            if i & 15 == 15 {
                cx.poll()?;
            }
        }
        Ok(())
    }
    /// # Safety
    /// `desc` and `desc+1` describe the output List<U> and its buffer.
    pub unsafe fn map<U: Slot>(
        self,
        cx: &mut Cx,
        desc: u32,
        mut f: impl FnMut(T) -> R<U>,
    ) -> R<List<U>> {
        let snapshot = self.slice(cx, 0, self.len(cx))?;
        let result = unsafe { List::<U>::with_capacity(cx, desc, snapshot.len(cx) as usize) }?;
        for i in 0..snapshot.len(cx) {
            let value = f(snapshot.get(cx, i)?)?;
            result.push(cx, value)?;
            if i & 15 == 15 {
                cx.poll()?;
            }
        }
        Ok(result)
    }
    pub fn filter(self, cx: &mut Cx, mut predicate: impl FnMut(T) -> R<bool>) -> R<Self> {
        let snapshot = self.slice(cx, 0, self.len(cx))?;
        let desc = unsafe { (*(self.pointer as *const Object)).header.desc };
        let result = unsafe { Self::new(cx, desc) }?;
        for i in 0..snapshot.len(cx) {
            let value = snapshot.get(cx, i)?;
            if predicate(value)? {
                result.push(cx, value)?;
            }
            if i & 15 == 15 {
                cx.poll()?;
            }
        }
        Ok(result)
    }
    fn find_pair(
        self,
        cx: &mut Cx,
        last: bool,
        mut predicate: impl FnMut(T) -> R<bool>,
    ) -> R<Option<(i64, T)>> {
        let snapshot = self.slice(cx, 0, self.len(cx))?;
        let len = snapshot.len(cx);
        for offset in 0..len {
            let i = if last { len - offset - 1 } else { offset };
            let value = snapshot.get(cx, i)?;
            if predicate(value)? {
                return Ok(Some((i, value)));
            }
            if offset & 15 == 15 {
                cx.poll()?;
            }
        }
        Ok(None)
    }
    pub fn find_index(
        self,
        cx: &mut Cx,
        last: bool,
        predicate: impl FnMut(T) -> R<bool>,
    ) -> R<Option<i64>> {
        Ok(self.find_pair(cx, last, predicate)?.map(|(i, _)| i))
    }
    pub fn find(
        self,
        cx: &mut Cx,
        last: bool,
        predicate: impl FnMut(T) -> R<bool>,
    ) -> R<Option<T>> {
        Ok(self.find_pair(cx, last, predicate)?.map(|(_, v)| v))
    }
    pub fn some(self, cx: &mut Cx, predicate: impl FnMut(T) -> R<bool>) -> R<bool> {
        Ok(self.find_pair(cx, false, predicate)?.is_some())
    }
    pub fn every(self, cx: &mut Cx, mut predicate: impl FnMut(T) -> R<bool>) -> R<bool> {
        Ok(self
            .find_pair(cx, false, |v| predicate(v).map(|b| !b))?
            .is_none())
    }
    /// # Safety
    /// `desc` and `desc+1` describe the output List<U> and its buffer.
    pub unsafe fn filter_map<U: Slot>(
        self,
        cx: &mut Cx,
        desc: u32,
        mut f: impl FnMut(T) -> R<Option<U>>,
    ) -> R<List<U>> {
        let snapshot = self.slice(cx, 0, self.len(cx))?;
        let output = unsafe { List::<U>::new(cx, desc) }?;
        for i in 0..snapshot.len(cx) {
            if let Some(value) = f(snapshot.get(cx, i)?)? {
                output.push(cx, value)?;
            }
            if i & 15 == 15 {
                cx.poll()?;
            }
        }
        Ok(output)
    }
    /// # Safety
    /// `desc` and `desc+1` describe the output List<U> and its buffer.
    pub unsafe fn flat_map<U: Slot>(
        self,
        cx: &mut Cx,
        desc: u32,
        mut f: impl FnMut(T) -> R<List<U>>,
    ) -> R<List<U>> {
        let snapshot = self.slice(cx, 0, self.len(cx))?;
        let output = unsafe { List::<U>::new(cx, desc) }?;
        for i in 0..snapshot.len(cx) {
            let list = f(snapshot.get(cx, i)?)?;
            for j in 0..list.len(cx) {
                let value = list.get(cx, j)?;
                output.push(cx, value)?;
            }
            if i & 15 == 15 {
                cx.poll()?;
            }
        }
        Ok(output)
    }
    pub fn reduce<A>(self, cx: &mut Cx, mut initial: A, mut f: impl FnMut(A, T) -> R<A>) -> R<A> {
        let snapshot = self.slice(cx, 0, self.len(cx))?;
        for i in 0..snapshot.len(cx) {
            initial = f(initial, snapshot.get(cx, i)?)?;
            if i & 15 == 15 {
                cx.poll()?;
            }
        }
        Ok(initial)
    }
    /// # Safety
    /// `key_desc` and `int_desc` each name a matching list/buffer descriptor pair.
    pub unsafe fn sort_by_key<K: Slot>(
        self,
        cx: &mut Cx,
        key_desc: u32,
        int_desc: u32,
        mut key: impl FnMut(T) -> R<K>,
        mut compare: impl FnMut(K, K) -> R<std::cmp::Ordering>,
    ) -> R<Self> {
        let len = self.len(cx);
        if len <= 1 {
            return Ok(self);
        }
        let snapshot = self.slice(cx, 0, len)?;
        let keys = unsafe { List::<K>::with_capacity(cx, key_desc, len as usize) }?;
        let order = unsafe { List::<i64>::with_capacity(cx, int_desc, len as usize) }?;
        for i in 0..len {
            let value = key(snapshot.get(cx, i)?)?;
            keys.push(cx, value)?;
            order.push(cx, i)?;
            if i & 15 == 15 {
                cx.poll()?;
            }
        }
        let read_cx = unsafe { Cx::current() };
        order.sort_by(cx, |a, b| {
            compare(keys.get(&read_cx, a)?, keys.get(&read_cx, b)?)
        })?;
        let desc = unsafe { (*(self.pointer as *const Object)).header.desc };
        let result = unsafe { Self::with_capacity(cx, desc, len as usize) }?;
        for i in 0..len {
            let value = snapshot.get(cx, order.get(cx, i)?)?;
            result.push(cx, value)?;
        }
        self.replace_contents(cx, result)?;
        Ok(self)
    }
    /// Stable bottom-up merge sort. Managed scratch lists keep every pointer
    /// reachable across comparator calls; write-back occurs only after success.
    pub fn sort_by(
        self,
        cx: &mut Cx,
        mut compare: impl FnMut(T, T) -> R<std::cmp::Ordering>,
    ) -> R<Self> {
        let len = self.len(cx);
        if len <= 1 {
            return Ok(self);
        }
        let mut source = self.slice(cx, 0, len)?;
        let mut merged = self.slice(cx, 0, len)?;
        let mut width = 1;
        while width < len {
            let mut start = 0;
            while start < len {
                let mid = (start + width).min(len);
                let end = (mid + width).min(len);
                let (mut left, mut right) = (start, mid);
                for at in start..end {
                    let take_left = right == end
                        || (left < mid
                            && compare(source.get(cx, left)?, source.get(cx, right)?)?
                                != std::cmp::Ordering::Greater);
                    let from = if take_left {
                        let i = left;
                        left += 1;
                        i
                    } else {
                        let i = right;
                        right += 1;
                        i
                    };
                    merged.set(cx, at, source.get(cx, from)?)?;
                    if at & 15 == 15 {
                        cx.poll()?;
                    }
                }
                start = end;
            }
            std::mem::swap(&mut source, &mut merged);
            width = width.saturating_mul(2);
        }
        self.replace_contents(cx, source)?;
        Ok(self)
    }
}

impl List<i64> {
    pub fn sum(self, cx: &Cx) -> R<i64> {
        let mut total = 0;
        for i in 0..self.len(cx) {
            total = crate::add(total, self.get(cx, i)?)?;
        }
        Ok(total)
    }
}
impl List<f64> {
    pub fn sum(self, cx: &Cx) -> f64 {
        // Iterator::sum preserves the VM's floating-point reduction order,
        // including the sign of the empty sum and NaN propagation.
        (0..self.len(cx))
            .map(|i| self.get(cx, i).expect("in-bounds sum"))
            .sum()
    }
    pub fn median(self, cx: &mut Cx) -> R<f64> {
        if self.is_empty(cx) {
            return Err(crate::list_errors::invalid_argument(
                cx,
                "float[].median: cannot take the median of an empty array".into(),
            ));
        }
        // Scalars need no managed roots; the receiver remains unchanged.
        let mut sorted = (0..self.len(cx))
            .map(|i| self.get(cx, i).expect("in-bounds median"))
            .collect::<Vec<_>>();
        sorted.sort_by(|a, b| crate::float_cmp(*a, *b));
        let middle = sorted.len() / 2;
        Ok(if sorted.len() % 2 == 1 {
            sorted[middle]
        } else {
            f64::midpoint(sorted[middle - 1], sorted[middle])
        })
    }
}

/// The VM's fixed list-search equality, independent of user `Equals` overrides.
pub fn builtin_eq(cx: &Cx, a: crate::Dyn, b: crate::Dyn) -> bool {
    if a.tag != b.tag {
        return false;
    }
    match a.tag {
        3 => {
            if a.payload == 0 || b.payload == 0 {
                a.payload == b.payload
            } else {
                unsafe { Str::from_raw(a.payload as usize) }.text(cx)
                    == unsafe { Str::from_raw(b.payload as usize) }.text(cx)
            }
        }
        tag if tag == (u32::MAX - 1) as u64 => {
            bex_vm_types::float_order::eq(f64::from_bits(a.payload), f64::from_bits(b.payload))
        }
        _ => a.payload == b.payload,
    }
}
pub fn primitive_string(cx: &mut Cx, value: crate::Dyn) -> R<Str> {
    let text = match value.tag {
        0 => (value.payload as i64).to_string(),
        1 => (value.payload != 0).to_string(),
        2 => "null".into(),
        3 => {
            if value.payload == 0 {
                "null".into()
            } else {
                return Ok(unsafe { Str::from_raw(value.payload as usize) });
            }
        }
        tag if tag == (u32::MAX - 1) as u64 => {
            bex_vm_types::format_float(f64::from_bits(value.payload))
        }
        _ => {
            return Err(crate::Error::internal(
                "non-primitive list join reached primitive adapter".into(),
            ));
        }
    };
    cx.string(&text)
}
/// Closed-world structural rendering metadata for list join. Field offsets,
/// types and taggedness come from the same descriptors used by precise tracing.
pub enum RenderShape {
    Unsupported,
    Class(&'static str),
    List { tag: u64, tagged: bool },
    StringIntMap,
}
/// # Safety
/// `value`, descriptors and shapes use the active generated program's ABI.
pub unsafe fn structural_string(
    cx: &mut Cx,
    value: crate::Dyn,
    descriptors: &'static [Descriptor],
    shapes: &'static [RenderShape],
) -> R<Str> {
    unsafe fn render(
        cx: &Cx,
        value: crate::Dyn,
        nested: bool,
        descriptors: &[Descriptor],
        shapes: &[RenderShape],
        text: &mut String,
    ) -> R<()> {
        crate::check_stack()?;
        match value.tag {
            0 => text.push_str(&(value.payload as i64).to_string()),
            1 => text.push_str(if value.payload == 0 { "false" } else { "true" }),
            2 => text.push_str("null"),
            3 => {
                if value.payload == 0 {
                    text.push_str("null");
                } else {
                    let string = unsafe { Str::from_raw(value.payload as usize) };
                    if nested {
                        use std::fmt::Write;
                        write!(text, "{:?}", string.try_as_str(cx)?).unwrap();
                    } else {
                        text.push_str(string.try_as_str(cx)?);
                    }
                }
            }
            tag if tag == (u32::MAX - 1) as u64 => {
                text.push_str(&crate::format_float(f64::from_bits(value.payload)))
            }
            tag => {
                if value.payload == 0 {
                    text.push_str("null");
                    return Ok(());
                }
                let desc = (tag - 3) as usize;
                let pointer = value.payload as usize as *const u8;
                match shapes.get(desc).unwrap_or(&RenderShape::Unsupported) {
                    RenderShape::Class(name) => {
                        text.push_str(name);
                        let fields = descriptors[desc].fields;
                        if !fields.is_empty() {
                            text.push_str(" { ");
                            for (i, field) in fields.iter().enumerate() {
                                if i != 0 {
                                    text.push_str(", ");
                                }
                                text.push_str(field.name);
                                text.push_str(": ");
                                let slot = unsafe { pointer.add(field.offset) };
                                let value = unsafe {
                                    if matches!(field.slot, baml_rt_gc::SlotKind::Tagged) {
                                        slot.cast::<crate::Dyn>().read()
                                    } else {
                                        crate::Dyn {
                                            tag: field.type_id as u64,
                                            payload: slot.cast::<u64>().read(),
                                        }
                                    }
                                };
                                unsafe { render(cx, value, true, descriptors, shapes, text) }?;
                            }
                            text.push_str(" }");
                        }
                    }
                    RenderShape::List { tag, tagged } => {
                        let list = pointer.cast::<usize>();
                        let (data, len) =
                            unsafe { ((list.add(1).read() + 16) as *const u8, list.add(2).read()) };
                        text.push('[');
                        for i in 0..len {
                            if i != 0 {
                                text.push_str(", ");
                            }
                            let value = unsafe {
                                if *tagged {
                                    data.cast::<crate::Dyn>().add(i).read()
                                } else {
                                    crate::Dyn {
                                        tag: *tag,
                                        payload: data.cast::<u64>().add(i).read(),
                                    }
                                }
                            };
                            unsafe { render(cx, value, true, descriptors, shapes, text) }?;
                        }
                        text.push(']');
                    }
                    RenderShape::StringIntMap => text.push_str(
                        &unsafe { crate::maps::StringIntMap::from_raw(value.payload as usize) }
                            .structural_string(cx),
                    ),
                    RenderShape::Unsupported => {
                        return Err(crate::Error::internal("unsupported list join shape".into()));
                    }
                }
            }
        }
        Ok(())
    }
    let mut text = String::new();
    unsafe { render(cx, value, false, descriptors, shapes, &mut text) }?;
    cx.string(&text)
}

pub fn join_with<T: Slot>(
    cx: &mut Cx,
    list: List<T>,
    separator: Str,
    mut render: impl FnMut(T) -> R<Str>,
) -> R<Str> {
    let snapshot = list.slice(cx, 0, list.len(cx))?;
    let mut text = String::new();
    for i in 0..snapshot.len(cx) {
        if i != 0 {
            text.push_str(separator.try_as_str(cx)?);
        }
        let value = render(snapshot.get(cx, i)?)?;
        text.push_str(value.try_as_str(cx)?);
        if i & 15 == 15 {
            cx.poll()?;
        }
    }
    cx.string(&text)
}

/// Nullable string elements use opaque pointer slots, so a null never becomes
/// a `Str` handle (whose safe readers require a live string).
pub fn join_pointers(cx: &mut Cx, list: List<Ptr>, separator: Str) -> R<Str> {
    let mut text = String::new();
    for i in 0..list.len(cx) {
        if i != 0 {
            text.push_str(separator.try_as_str(cx)?);
        }
        let value = list.get(cx, i)?.as_raw();
        if value == 0 {
            text.push_str("null");
        } else {
            text.push_str(unsafe { Str::from_raw(value) }.try_as_str(cx)?);
        }
        if i & 15 == 15 {
            cx.poll()?;
        }
    }
    cx.string(&text)
}

pub fn join(cx: &mut Cx, list: List<Str>, separator: Str) -> R<Str> {
    let mut text = String::new();
    for i in 0..list.len(cx) {
        if i != 0 {
            text.push_str(separator.try_as_str(cx)?);
        }
        let value = list.get(cx, i)?;
        text.push_str(if value.as_raw() == 0 {
            "null"
        } else {
            value.try_as_str(cx)?
        });
        if i & 15 == 15 {
            cx.poll()?;
        }
    }
    cx.string(&text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Instance, language::*};
    static DESCRIPTORS: &[Descriptor] = &[
        STRING_DESC,
        OVERFLOW_DESC,
        SHIFT_DESC,
        CANCEL_DESC,
        STACK_DESC,
        ALLOC_DESC,
        INDEX_DESC,
        descriptor::<Str>(10),
        buffer_descriptor::<Str>(11),
        descriptor::<List<Str>>(12),
        buffer_descriptor::<List<Str>>(13),
        descriptor::<i64>(14),
        buffer_descriptor::<i64>(15),
        descriptor::<crate::Dyn>(16),
        buffer_descriptor::<crate::Dyn>(17),
        descriptor::<f64>(18),
        buffer_descriptor::<f64>(19),
        descriptor::<bool>(20),
        buffer_descriptor::<bool>(21),
    ];
    const POINTER_TYPES: &[bool] = &[
        false, false, false, true, true, true, true, true, true, true, true, true, true, true,
        true, true, true, true, true, true, true, true,
    ];
    #[test]
    fn scalar_and_tagged_mutation_slice_and_gc() {
        fn entry() -> R<i64> {
            let mut cx = unsafe { Cx::current() };
            let floats = unsafe { List::<f64>::new(&mut cx, 15) }?;
            let bools = unsafe { List::<bool>::new(&mut cx, 17) }?;
            let mixed = unsafe { List::<crate::Dyn>::new(&mut cx, 13) }?;
            for i in 0..100 {
                floats.push(&mut cx, i as f64 + 0.5)?;
                bools.push(&mut cx, i % 2 == 0)?;
                let text = cx.string(&i.to_string())?;
                mixed.push(
                    &mut cx,
                    crate::Dyn {
                        tag: 3,
                        payload: text.as_raw() as u64,
                    },
                )?;
                mixed.push(
                    &mut cx,
                    crate::Dyn {
                        tag: 0,
                        payload: i as u64,
                    },
                )?;
                cx.poll()?;
            }
            floats.set(&cx, -1, -0.0)?;
            assert_eq!(floats.get(&cx, -1)?.to_bits(), (-0.0f64).to_bits());
            assert!(bools.get(&cx, 98)?);
            assert!(!bools.get(&cx, 99)?);
            assert_eq!(mixed.len(&cx), 200);
            let snapshot = mixed.slice(&mut cx, -4, 999)?;
            let reversed = snapshot.reverse(&mut cx)?;
            mixed.clear(&cx);
            cx.poll()?;
            assert_eq!(snapshot.len(&cx), 4);
            assert_eq!(reversed.get(&cx, 0)?.payload, 99);
            let last = snapshot.get(&cx, -2)?;
            assert_eq!(
                unsafe { Str::from_raw(last.payload as usize) }.as_str(&cx),
                "99"
            );
            let joined = snapshot.concat(&mut cx, snapshot)?;
            assert_eq!(joined.len(&cx), 8);
            assert_eq!(joined.remove_at(&cx, -1).unwrap().payload, 99);
            assert!(joined.at(&cx, 7).is_none());
            assert!(joined.slice(&mut cx, 5, 1)?.is_empty(&cx));
            assert!(floats.set(&cx, 100, 1.0).is_err());
            Ok(200)
        }
        let mut instance = Instance::new(DESCRIPTORS, POINTER_TYPES);
        instance.set_stress(true);
        assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 200);
        assert!(instance.stats().collections > 100);
    }
    #[test]
    fn callback_snapshots_stable_sort_and_atomic_error() {
        fn entry() -> R<i64> {
            let mut cx = unsafe { Cx::current() };
            let input = unsafe { List::<i64>::new(&mut cx, 11) }?;
            for i in [31, 12, 23, 14] {
                input.push(&mut cx, i)?;
            }
            let mapped = unsafe {
                input.map(&mut cx, 7, |value| {
                    let mut callback_cx = Cx::current();
                    input.push(&mut callback_cx, 999)?;
                    callback_cx.string(&value.to_string())
                })
            }?;
            assert_eq!(mapped.len(&cx), 4);
            assert_eq!(input.len(&cx), 8);
            let snapshot = input.slice(&mut cx, 0, 4)?;
            snapshot.sort_by(&mut cx, |a, b| Ok((a / 10).cmp(&(b / 10))))?;
            assert_eq!((snapshot.get(&cx, 0)?, snapshot.get(&cx, 1)?), (12, 14));
            assert_eq!((snapshot.get(&cx, 2)?, snapshot.get(&cx, 3)?), (23, 31));
            let before = snapshot.slice(&mut cx, 0, 99)?;
            let mut comparisons = 0;
            let error = snapshot.sort_by(&mut cx, |a, b| {
                comparisons += 1;
                if comparisons == 2 {
                    return Err(crate::Error::internal("stop".into()));
                }
                Ok(b.cmp(&a))
            });
            assert!(error.is_err());
            for i in 0..4 {
                assert_eq!(snapshot.get(&cx, i)?, before.get(&cx, i)?);
            }
            let selected = input.filter(&mut cx, |v| Ok(v < 100))?;
            assert_eq!(selected.len(&cx), 4);
            assert_eq!(selected.reduce(&mut cx, 0, |a, b| Ok(a + b))?, 80);
            let mut calls = 0;
            assert_eq!(
                selected.find(&mut cx, false, |v| {
                    calls += 1;
                    Ok(v < 20)
                })?,
                Some(12)
            );
            assert_eq!(calls, 2);
            assert_eq!(
                selected.find_index(&mut cx, true, |v| Ok(v % 2 == 0))?,
                Some(3)
            );
            assert!(selected.every(&mut cx, |v| Ok(v < 100))?);
            assert!(!selected.some(&mut cx, |v| Ok(v > 100))?);
            let strings = unsafe {
                selected.filter_map(&mut cx, 7, |v| {
                    if v % 2 == 0 {
                        Ok(Some(Cx::current().string(&v.to_string())?))
                    } else {
                        Ok(None)
                    }
                })
            }?;
            assert_eq!(strings.len(&cx), 2);
            assert_eq!(strings.get(&cx, 1)?.as_str(&cx), "14");
            let expanded = unsafe {
                selected.flat_map(&mut cx, 11, |v| {
                    let mut callback_cx = Cx::current();
                    let pair = List::<i64>::new(&mut callback_cx, 11)?;
                    pair.push(&mut callback_cx, v)?;
                    pair.push(&mut callback_cx, v + 1)?;
                    Ok(pair)
                })
            }?;
            assert_eq!(expanded.len(&cx), 8);
            assert_eq!(expanded.sum(&cx)?, 164);
            let mut keys = 0;
            let read_cx = unsafe { Cx::current() };
            unsafe {
                selected.sort_by_key(
                    &mut cx,
                    7,
                    11,
                    |v| {
                        keys += 1;
                        Cx::current().string(&(v / 10).to_string())
                    },
                    |a: Str, b: Str| Ok(a.as_str(&read_cx).cmp(b.as_str(&read_cx))),
                )
            }?;
            assert_eq!(keys, 4);
            assert_eq!((selected.get(&cx, 0)?, selected.get(&cx, 1)?), (12, 14));
            let singleton = unsafe { List::<i64>::filled(&mut cx, 11, 1, 7) }?;
            unsafe {
                singleton.sort_by_key(
                    &mut cx,
                    11,
                    11,
                    |_| -> R<i64> { panic!("singleton sort must not call key") },
                    |a, b| Ok(a.cmp(&b)),
                )
            }?;
            let generated = unsafe {
                List::<Str>::generate(&mut cx, 7, 8, |i| Cx::current().string(&i.to_string()))
            }?;
            assert_eq!(generated.get(&cx, -1)?.as_str(&cx), "7");
            Ok(4)
        }
        let mut instance = Instance::new(DESCRIPTORS, POINTER_TYPES);
        instance.set_stress(true);
        assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 4);
    }
    #[test]
    fn embedded_buffers_trace_grow_and_clear_without_detaching_aliases() {
        fn entry() -> R<i64> {
            let mut cx = unsafe { Cx::current() };
            let strings = unsafe { List::<Str>::with_capacity(&mut cx, 7, 4) }?;
            let mixed = unsafe { List::<crate::Dyn>::with_capacity(&mut cx, 13, 4) }?;
            for i in 0..4 {
                let text = cx.string(&format!("item{i}"))?;
                strings.push(&mut cx, text)?;
                mixed.push(
                    &mut cx,
                    crate::Dyn {
                        tag: 3,
                        payload: text.as_raw() as u64,
                    },
                )?;
            }
            assert_eq!(
                unsafe { (*(strings.pointer as *const Object)).buffer },
                strings.pointer + 16
            );
            cx.poll()?;
            assert_eq!(strings.get(&cx, -1)?.as_str(&cx), "item3");
            let alias = strings;
            let replacement = strings.reverse(&mut cx)?;
            strings.replace_contents(&mut cx, replacement)?;
            cx.poll()?;
            assert_eq!(alias.get(&cx, 0)?.as_str(&cx), "item3");
            let text = cx.string("growth")?;
            strings.push(&mut cx, text)?;
            mixed.push(
                &mut cx,
                crate::Dyn {
                    tag: 3,
                    payload: text.as_raw() as u64,
                },
            )?;
            unsafe {
                // The embedded tail remains part of the shell's precise map.
                // Growth must clear it so old elements do not stay retained.
                for i in 0..4 {
                    assert_eq!(((strings.pointer + 32 + i * 8) as *const usize).read(), 0);
                    let slot = ((mixed.pointer + 32 + i * 16) as *const crate::Dyn).read();
                    assert_eq!((slot.tag, slot.payload), (2, 0));
                }
            }
            cx.poll()?;
            assert_eq!(alias.get(&cx, -1)?.as_str(&cx), "growth");
            let tail = mixed.get(&cx, -1)?;
            assert_eq!(
                unsafe { Str::from_raw(tail.payload as usize) }.as_str(&cx),
                "growth"
            );
            strings.clear(&cx);
            mixed.clear(&cx);
            cx.poll()?;
            assert!(alias.is_empty(&cx));
            let nullable = unsafe { List::<Ptr>::with_capacity(&mut cx, 7, 3) }?;
            let word = cx.string("present")?;
            nullable.push(&mut cx, unsafe { Ptr::from_raw(word.as_raw()) })?;
            nullable.push(&mut cx, unsafe { Ptr::from_raw(0) })?;
            cx.poll()?;
            let separator = cx.string("|")?;
            assert_eq!(
                join_pointers(&mut cx, nullable, separator)?.as_str(&cx),
                "present|null"
            );
            Ok(1)
        }
        let mut instance = Instance::new(DESCRIPTORS, POINTER_TYPES);
        instance.set_stress(true);
        assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 1);
    }
    #[test]
    fn failed_growth_preserves_length_and_elements() {
        fn entry() -> R<i64> {
            let mut cx = unsafe { Cx::current() };
            let list = unsafe { List::<i64>::new(&mut cx, 11) }?;
            let mut inserted = 0;
            loop {
                match list.push(&mut cx, inserted) {
                    Ok(_) => inserted += 1,
                    Err(error) => {
                        assert_eq!(error.class, "baml.panics.AllocFailure");
                        break;
                    }
                }
                assert!(
                    inserted < 4096,
                    "configured limit must force growth failure"
                );
            }
            assert!(inserted > 0);
            assert_eq!(list.len(&cx), inserted);
            for i in 0..inserted {
                assert_eq!(list.get(&cx, i)?, i);
            }
            Ok(inserted)
        }
        let mut instance = Instance::new(DESCRIPTORS, POINTER_TYPES);
        instance.set_heap_limit(32768);
        instance.set_stress(true);
        assert!(unsafe { instance.run_baml(entry) }.unwrap() > 0);
    }
    #[test]
    fn growth_aliases_nested_lists_and_join_survive_stress() {
        fn entry() -> R<i64> {
            let mut cx = unsafe { Cx::current() };
            let outer = unsafe { List::<List<Str>>::new(&mut cx, 9) }?;
            let inner = unsafe { List::<Str>::new(&mut cx, 7) }?;
            outer.push(&mut cx, inner)?;
            let alias = outer.get(&cx, 0)?;
            for i in 0..65 {
                let text = cx.string(&i.to_string())?;
                assert_eq!(inner.push(&mut cx, text)?, i + 1);
                cx.poll()?;
            }
            assert_eq!(alias.len(&cx), 65);
            assert_eq!(alias.get(&cx, -1)?.as_str(&cx), "64");
            let sep = cx.string("é")?;
            let joined = join(&mut cx, alias, sep)?;
            let expected = (0..65).map(|i| i.to_string()).collect::<Vec<_>>().join("é");
            assert_eq!(joined.as_str(&cx), expected);
            assert!(alias.get(&cx, 65).is_err());
            assert!(alias.get(&cx, -66).is_err());
            Ok(65)
        }
        let mut instance = Instance::new(DESCRIPTORS, POINTER_TYPES);
        instance.set_stress(true);
        assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 65);
        assert!(instance.stats().collections >= 65);
    }
}
