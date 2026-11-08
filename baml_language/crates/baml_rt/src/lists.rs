//! Typed growing lists. The stable list object owns a separately traced buffer;
//! growth replaces that pointer, so aliases continue to name the same list.
use crate::{
    R,
    language::{allocation_failure, index_error},
    strings::{Cx, Str},
};
use baml_rt_gc::{Descriptor, ElementLayout, Elements, Header, Kind, write_ptr};
use std::{marker::PhantomData, rc::Rc};

mod sealed {
    pub trait Sealed {}
}
/// Supported eight-byte elements. Pointer writes use the heap hook.
pub trait Slot: sealed::Sealed + Copy {
    const POINTER: bool;
    fn word(self) -> usize;
    /// # Safety
    /// The word is a valid, rooted value of this slot type in the active instance.
    unsafe fn from_word(word: usize) -> Self;
}
impl sealed::Sealed for i64 {}
impl Slot for i64 {
    const POINTER: bool = false;
    fn word(self) -> usize {
        self as usize
    }
    unsafe fn from_word(word: usize) -> Self {
        word as i64
    }
}
impl sealed::Sealed for Str {}
impl Slot for Str {
    const POINTER: bool = true;
    fn word(self) -> usize {
        self.as_raw()
    }
    unsafe fn from_word(word: usize) -> Self {
        unsafe { Self::from_raw(word) }
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
impl<T: Slot> sealed::Sealed for List<T> {}
impl<T: Slot> Slot for List<T> {
    const POINTER: bool = true;
    fn word(self) -> usize {
        self.pointer
    }
    unsafe fn from_word(word: usize) -> Self {
        unsafe { Self::from_raw(word) }
    }
}

/// The compiler emits this descriptor and then the corresponding buffer descriptor.
pub const fn descriptor(type_id: u32) -> Descriptor {
    Descriptor {
        kind: Kind::List,
        size: 32,
        ptr_slots: (8, 1),
        tagged_slots: (0, 0),
        elem: None,
        type_id,
        fields: &[],
        finalizer: None,
    }
}
pub const fn buffer_descriptor<T: Slot>(type_id: u32) -> Descriptor {
    Descriptor {
        kind: Kind::List,
        size: 8,
        ptr_slots: (0, 0),
        tagged_slots: (0, 0),
        elem: Some(Elements {
            offset: 16,
            layout: if T::POINTER {
                ElementLayout::Pointer
            } else {
                ElementLayout::Scalar
            },
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
    pub unsafe fn new(_cx: &mut Cx, desc: u32) -> R<Self> {
        let pointer = unsafe { crate::try_alloc(desc, 32) }? as usize;
        Ok(unsafe { Self::from_raw(pointer) })
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
    pub fn len(self, _cx: &Cx) -> i64 {
        unsafe { (*(self.pointer as *const Object)).len as i64 }
    }
    pub fn is_empty(self, cx: &Cx) -> bool {
        self.len(cx) == 0
    }
    pub fn get(self, cx: &Cx, index: i64) -> R<T> {
        let len = self.len(cx);
        let resolved = if index < 0 { index + len } else { index };
        if resolved < 0 || resolved >= len {
            return Err(index_error(index, len as usize));
        }
        unsafe {
            let buffer = (*(self.pointer as *const Object)).buffer;
            Ok(T::from_word(
                ((buffer + 16) as *const usize)
                    .add(resolved as usize)
                    .read(),
            ))
        }
    }
    /// Independent snapshot of the mutable container; immutable element values
    /// may be shared. Used by memoized string segmentation.
    #[inline(always)]
    pub fn copy(self, cx: &mut Cx) -> R<Self> {
        let old = self.pointer as *const Object;
        let (len, desc) = unsafe { ((*old).len, (*old).header.desc) };
        if len == 0 {
            return unsafe { Self::new(cx, desc) };
        }
        let bytes = len
            .checked_mul(8)
            .and_then(|n| n.checked_add(16))
            .ok_or_else(|| allocation_failure(usize::MAX))?;
        let buffer = cx.alloc(desc + 1, bytes)?;
        unsafe {
            let source = (*old).buffer as *const u8;
            for i in 0..len {
                let word = source.add(16).cast::<usize>().add(i).read();
                let slot = buffer.add(16).cast::<usize>().add(i);
                if T::POINTER {
                    write_ptr(buffer, slot, word)
                } else {
                    slot.write(word)
                }
            }
        }
        let result = cx.alloc(desc, 32)?;
        unsafe {
            let object = &mut *result.cast::<Object>();
            write_ptr(result, &raw mut object.buffer, buffer as usize);
            object.len = len;
            object.capacity = len;
        }
        Ok(unsafe { Self::from_raw(result as usize) })
    }
    pub fn push(self, _cx: &mut Cx, value: T) -> R<i64> {
        // Never retain a Rust borrow of the list across a collecting allocation.
        let object = self.pointer as *mut Object;
        let (len, capacity, desc) =
            unsafe { ((*object).len, (*object).capacity, (*object).header.desc) };
        if len == capacity {
            let capacity = capacity
                .checked_mul(2)
                .map(|n| n.max(4))
                .filter(|n| *n <= baml_type::Int63::MAX.get() as usize)
                .ok_or_else(|| allocation_failure(usize::MAX))?;
            let bytes = capacity
                .checked_mul(8)
                .and_then(|n| n.checked_add(16))
                .ok_or_else(|| allocation_failure(usize::MAX))?;
            let buffer = unsafe { crate::try_alloc(desc + 1, bytes) }?;
            unsafe {
                let old = (*object).buffer;
                for i in 0..len {
                    let word = ((old + 16) as *const usize).add(i).read();
                    let slot = buffer.add(16).cast::<usize>().add(i);
                    if T::POINTER {
                        write_ptr(buffer, slot, word);
                    } else {
                        slot.write(word);
                    }
                }
                write_ptr(object.cast(), &raw mut (*object).buffer, buffer as usize);
                (*object).capacity = capacity;
            }
        }
        unsafe {
            let buffer = (*object).buffer as *mut u8;
            let slot = buffer.add(16).cast::<usize>().add(len);
            if T::POINTER {
                write_ptr(buffer, slot, value.word());
            } else {
                slot.write(value.word());
            }
            (*object).len = len + 1;
        }
        Ok((len + 1) as i64)
    }
}

pub fn join(cx: &mut Cx, list: List<Str>, separator: Str) -> R<Str> {
    let mut text = String::new();
    for i in 0..list.len(cx) {
        if i != 0 {
            text.push_str(separator.try_as_str(cx)?);
        }
        text.push_str(list.get(cx, i)?.try_as_str(cx)?);
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
        descriptor(10),
        buffer_descriptor::<Str>(11),
        descriptor(12),
        buffer_descriptor::<List<Str>>(13),
        descriptor(14),
        buffer_descriptor::<i64>(15),
    ];
    const POINTER_TYPES: &[bool] = &[
        false, false, false, true, true, true, true, true, true, true, true, true, true, true,
        true, true,
    ];
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
