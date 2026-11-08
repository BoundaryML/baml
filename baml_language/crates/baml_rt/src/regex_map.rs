//! Insertion-ordered string/group maps for regex captures. Keys and values are traced;
//! replacement keeps insertion order and values() makes an independent snapshot.
use crate::{
    R,
    language::allocation_failure,
    strings::{Cx, Str},
};
use baml_rt_gc::{Descriptor, ElementLayout, Elements, Header, Kind, write_ptr};
use std::{marker::PhantomData, rc::Rc};
#[repr(C)]
struct Object {
    header: Header,
    entries: usize,
    len: usize,
    capacity: usize,
}
#[repr(C)]
struct Entry {
    key: usize,
    value: usize,
}
#[derive(Clone, Copy)]
pub struct StringPtrMap {
    pointer: usize,
    owner: PhantomData<Rc<()>>,
}
pub const fn descriptors(desc: u32) -> [Descriptor; 3] {
    [
        Descriptor {
            kind: Kind::Map,
            size: 32,
            ptr_slots: (8, 1),
            tagged_slots: (0, 0),
            elem: None,
            type_id: desc + 3,
            fields: &[],
            finalizer: None,
        },
        Descriptor {
            kind: Kind::Map,
            size: 16,
            ptr_slots: (0, 0),
            tagged_slots: (0, 0),
            elem: Some(Elements {
                offset: 16,
                layout: ElementLayout::Descriptor(desc + 2),
            }),
            type_id: desc + 4,
            fields: &[],
            finalizer: None,
        },
        Descriptor {
            kind: Kind::InlineElement,
            size: 16,
            ptr_slots: (0, 2),
            tagged_slots: (0, 0),
            elem: None,
            type_id: desc + 5,
            fields: &[],
            finalizer: None,
        },
    ]
}
impl StringPtrMap {
    /// # Safety
    /// `desc..desc+3` is this module's descriptor triple. All handles and copies
    /// obey the active Cx's lifetime and rooting contract.
    pub unsafe fn new(_cx: &mut Cx, desc: u32) -> R<Self> {
        Ok(unsafe { Self::from_raw(crate::try_alloc(desc, 32)? as usize) })
    }
    /// # Safety
    /// A live map with the descriptor triple above, rooted in the active Cx.
    pub unsafe fn from_raw(pointer: usize) -> Self {
        Self {
            pointer,
            owner: PhantomData,
        }
    }
    pub fn has(self, cx: &Cx, key: Str) -> R<bool> {
        let object = self.pointer as *const Object;
        for i in 0..unsafe { (*object).len } {
            let entry = unsafe { (((*object).entries + 16) as *const Entry).add(i) };
            if crate::strings::equals(cx, unsafe { Str::from_raw((*entry).key) }, key)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub unsafe fn keys(self, cx: &mut Cx, desc: u32) -> R<usize> {
        let list = unsafe { crate::lists::List::<Str>::new(cx, desc)? };
        let object = self.pointer as *const Object;
        for i in 0..unsafe { (*object).len } {
            let entry = unsafe { (((*object).entries + 16) as *const Entry).add(i) };
            list.push(cx, unsafe { Str::from_raw((*entry).key) })?;
        }
        Ok(list.as_raw())
    }
    pub fn get(self, cx: &Cx, key: Str) -> R<usize> {
        let object = self.pointer as *const Object;
        for i in 0..unsafe { (*object).len } {
            let entry = unsafe { (((*object).entries + 16) as *const Entry).add(i) };
            if crate::strings::equals(cx, unsafe { Str::from_raw((*entry).key) }, key)? {
                return Ok(unsafe { (*entry).value });
            }
        }
        Ok(0)
    }
    pub fn len(self) -> i64 {
        unsafe { (*(self.pointer as *const Object)).len as i64 }
    }
    pub fn as_raw(self) -> usize {
        self.pointer
    }
    pub fn set(self, cx: &mut Cx, key: Str, value: usize) -> R<usize> {
        let object = self.pointer as *mut Object;
        let (len, capacity, desc) =
            unsafe { ((*object).len, (*object).capacity, (*object).header.desc) };
        for i in 0..len {
            unsafe {
                let entry = (((*object).entries + 16) as *mut Entry).add(i);
                if Str::from_raw((*entry).key).try_as_str(cx)? == key.try_as_str(cx)? {
                    let old = (*entry).value;
                    write_ptr((*object).entries as *mut u8, &raw mut (*entry).value, value);
                    return Ok(old);
                }
            }
        }
        if len == capacity {
            let capacity = capacity
                .checked_mul(2)
                .map(|n| n.max(4))
                .filter(|n| *n <= baml_type::Int63::MAX.get() as usize)
                .ok_or_else(|| allocation_failure(usize::MAX))?;
            let bytes = capacity
                .checked_mul(16)
                .and_then(|n| n.checked_add(16))
                .ok_or_else(|| allocation_failure(usize::MAX))?;
            let buffer = unsafe { crate::try_alloc(desc + 1, bytes) }?;
            unsafe {
                for i in 0..len {
                    let old = (((*object).entries + 16) as *const Entry).add(i);
                    let new = buffer.add(16).cast::<Entry>().add(i);
                    write_ptr(buffer, &raw mut (*new).key, (*old).key);
                    write_ptr(buffer, &raw mut (*new).value, (*old).value);
                }
                write_ptr(object.cast(), &raw mut (*object).entries, buffer as usize);
                (*object).capacity = capacity;
            }
        }
        unsafe {
            let buffer = (*object).entries as *mut u8;
            let entry = buffer.add(16).cast::<Entry>().add(len);
            write_ptr(buffer, &raw mut (*entry).key, key.as_raw());
            write_ptr(buffer, &raw mut (*entry).value, value);
            (*object).len = len + 1;
        }
        Ok(0)
    }
    /// # Safety
    /// `desc` describes the existing fixed pointer-list layout (length at 8, pointers
    /// elements at 16), and the result follows Cx's rooting contract.
    pub unsafe fn values(self, _cx: &mut Cx, desc: u32) -> R<usize> {
        let object = self.pointer as *const Object;
        let len = unsafe { (*object).len };
        let bytes = len
            .checked_mul(8)
            .and_then(|n| n.checked_add(16))
            .ok_or_else(|| allocation_failure(usize::MAX))?;
        let list = unsafe { crate::try_alloc(desc, bytes) }?;
        unsafe {
            list.add(8).cast::<usize>().write(len);
            for i in 0..len {
                let entry = (((*object).entries + 16) as *const Entry).add(i);
                write_ptr(list, list.add(16).cast::<usize>().add(i), (*entry).value);
            }
        }
        Ok(list as usize)
    }
}
