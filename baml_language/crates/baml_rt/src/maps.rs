//! insertion-ordered string/int maps. Entries and keys are managed;
//! replacement keeps insertion order and values() makes an independent snapshot.
use crate::{
    Dyn, R,
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
    value: i64,
}
#[derive(Clone, Copy)]
pub struct StringIntMap {
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
            ptr_slots: (0, 1),
            tagged_slots: (0, 0),
            elem: None,
            type_id: desc + 5,
            fields: &[],
            finalizer: None,
        },
    ]
}
impl StringIntMap {
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
    pub(crate) fn structural_string(self, cx: &Cx) -> String {
        use std::fmt::Write;
        let object = self.pointer as *const Object;
        let mut text = String::from("{");
        for i in 0..unsafe { (*object).len } {
            if i != 0 {
                text.push_str(", ");
            }
            let entry = unsafe { &*(((*object).entries + 16) as *const Entry).add(i) };
            let key = unsafe { Str::from_raw(entry.key) };
            write!(&mut text, "{:?}: {}", key.as_str(cx), entry.value).unwrap();
        }
        text.push('}');
        text
    }
    pub fn as_raw(self) -> usize {
        self.pointer
    }
    pub fn set(self, cx: &mut Cx, key: Str, value: i64) -> R<Dyn> {
        let object = self.pointer as *mut Object;
        let (len, capacity, desc) =
            unsafe { ((*object).len, (*object).capacity, (*object).header.desc) };
        for i in 0..len {
            unsafe {
                let entry = (((*object).entries + 16) as *mut Entry).add(i);
                if Str::from_raw((*entry).key).as_str(cx) == key.as_str(cx) {
                    let old = (*entry).value;
                    (*entry).value = value;
                    return Ok(Dyn {
                        tag: 0,
                        payload: old as u64,
                    });
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
                    (*new).value = (*old).value;
                }
                write_ptr(object.cast(), &raw mut (*object).entries, buffer as usize);
                (*object).capacity = capacity;
            }
        }
        unsafe {
            let buffer = (*object).entries as *mut u8;
            let entry = buffer.add(16).cast::<Entry>().add(len);
            write_ptr(buffer, &raw mut (*entry).key, key.as_raw());
            (*entry).value = value;
            (*object).len = len + 1;
        }
        Ok(Dyn { tag: 2, payload: 0 })
    }
    /// # Safety
    /// `desc` and `desc+1` describe a growing List<i64> and its buffer.
    pub unsafe fn growing_values(self, cx: &mut Cx, desc: u32) -> R<usize> {
        let object = self.pointer as *const Object;
        let len = unsafe { (*object).len };
        let list = unsafe { crate::lists::List::<i64>::with_capacity(cx, desc, len) }?;
        for i in 0..len {
            let value = unsafe { (*(((*object).entries + 16) as *const Entry).add(i)).value };
            list.push(cx, value)?;
        }
        Ok(list.as_raw())
    }
    /// # Safety
    /// `desc` describes the existing fixed int-list layout (length at 8, i64
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
                list.add(16).cast::<i64>().add(i).write((*entry).value);
            }
        }
        Ok(list as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Instance, language::*};
    static MAP: [Descriptor; 3] = descriptors(7);
    static DESCRIPTORS: &[Descriptor] = &[
        STRING_DESC,
        OVERFLOW_DESC,
        SHIFT_DESC,
        CANCEL_DESC,
        STACK_DESC,
        ALLOC_DESC,
        INDEX_DESC,
        MAP[0],
        MAP[1],
        MAP[2],
        Descriptor {
            kind: Kind::List,
            size: 8,
            ptr_slots: (0, 0),
            tagged_slots: (0, 0),
            elem: Some(Elements {
                offset: 16,
                layout: ElementLayout::Scalar,
            }),
            type_id: 13,
            fields: &[],
            finalizer: None,
        },
    ];
    #[test]
    fn replacement_order_and_snapshot_survive_gc() {
        fn entry() -> R<i64> {
            let mut cx = unsafe { Cx::current() };
            let map = unsafe { StringIntMap::new(&mut cx, 7) }?;
            for i in 0..65 {
                let key = cx.string(&format!("key-{i}"))?;
                assert_eq!(map.set(&mut cx, key, i)?.tag, 2);
                cx.poll()?;
            }
            let before = unsafe { map.values(&mut cx, 10) }?;
            let key = cx.string("key-3")?;
            let old = map.set(&mut cx, key, 900)?;
            assert_eq!((old.tag, old.payload), (0, 3));
            let after = unsafe { map.values(&mut cx, 10) }?;
            cx.poll()?;
            for i in 0..65 {
                assert_eq!(unsafe { *(crate::list_slot(before, i)? as *const i64) }, i);
                assert_eq!(
                    unsafe { *(crate::list_slot(after, i)? as *const i64) },
                    if i == 3 { 900 } else { i }
                );
            }
            Ok(65)
        }
        let mut instance = Instance::new(
            DESCRIPTORS,
            &[
                false, false, false, true, true, true, true, true, true, true, true, true, true,
                true,
            ],
        );
        instance.set_stress(true);
        assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 65);
    }
}
