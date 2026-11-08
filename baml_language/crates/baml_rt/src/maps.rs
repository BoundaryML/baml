//! Typed insertion-ordered maps. All key/value storage belongs to the managed
//! heap; the hash index contains scalar entry indices only.
pub mod json;
mod slots;
use crate::{
    Dyn, R,
    language::allocation_failure,
    strings::{Cx, Str},
};
use baml_rt_gc::{Descriptor, ElementLayout, Elements, Header, Kind, write_ptr};
pub use slots::{Key, Null, Pointer, Slot};
use std::{
    marker::PhantomData,
    mem::{offset_of, size_of},
    rc::Rc,
};
#[repr(C)]
struct Object {
    header: Header,
    entries: usize,
    buckets: usize,
    len: usize,
    capacity: usize,
    epoch: u64,
}
#[repr(C)]
struct Entry<K: Slot, V: Slot> {
    key: K,
    value: V,
    hash: u64,
}
#[repr(transparent)]
pub struct Map<K: Slot, V: Slot> {
    pointer: usize,
    owner: PhantomData<(K, V, Rc<()>)>,
}
impl<K: Slot, V: Slot> Copy for Map<K, V> {}
impl<K: Slot, V: Slot> Clone for Map<K, V> {
    fn clone(&self) -> Self {
        *self
    }
}
unsafe impl<K: Slot, V: Slot> Slot for Map<K, V> {
    const POINTER: bool = true;
    unsafe fn store(owner: *mut u8, target: *mut Self, value: Self) {
        unsafe { write_ptr(owner, target.cast(), value.pointer) };
    }
}
/// Four consecutive descriptors: map, entry buffer, inline entry, scalar index.
pub const fn typed_descriptors<K: Slot, V: Slot>(desc: u32) -> [Descriptor; 4] {
    let key = offset_of!(Entry<K,V>, key);
    let value = offset_of!(Entry<K,V>, value);
    let pointer_offset = if K::POINTER {
        key
    } else if V::POINTER {
        value
    } else {
        0
    };
    let tagged_offset = if K::TAGGED {
        key
    } else if V::TAGGED {
        value
    } else {
        0
    };
    let pointers = K::POINTER as usize + V::POINTER as usize;
    let tagged = K::TAGGED as usize + V::TAGGED as usize;
    [
        Descriptor {
            kind: Kind::Map,
            size: size_of::<Object>(),
            ptr_slots: (8, 2),
            tagged_slots: (0, 0),
            elem: None,
            type_id: desc + 3,
            fields: &[],
            finalizer: None,
        },
        Descriptor {
            kind: Kind::Map,
            size: size_of::<Entry<K, V>>(),
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
            size: size_of::<Entry<K, V>>(),
            ptr_slots: (pointer_offset, pointers),
            tagged_slots: (tagged_offset, tagged),
            elem: None,
            type_id: desc + 5,
            fields: &[],
            finalizer: None,
        },
        Descriptor {
            kind: Kind::List,
            size: 8,
            ptr_slots: (0, 0),
            tagged_slots: (0, 0),
            elem: Some(Elements {
                offset: 16,
                layout: ElementLayout::Scalar,
            }),
            type_id: desc + 6,
            fields: &[],
            finalizer: None,
        },
    ]
}
pub const fn descriptors(desc: u32) -> [Descriptor; 4] {
    typed_descriptors::<Str, i64>(desc)
}
impl<K: Slot, V: Slot> Map<K, V> {
    /// # Safety
    /// `desc..desc+4` must equal typed_descriptors::<K,V>. Handles must obey the
    /// active instance's lifetime and conservative stack rooting contract.
    pub unsafe fn new(_cx: &mut Cx, desc: u32) -> R<Self> {
        Ok(unsafe { Self::from_raw(crate::try_alloc(desc, size_of::<Object>())? as usize) })
    }
    /// # Safety
    /// The pointer names a live Map<K,V> in the active instance.
    pub unsafe fn from_raw(pointer: usize) -> Self {
        Self {
            pointer,
            owner: PhantomData,
        }
    }
    pub fn as_raw(self) -> usize {
        self.pointer
    }
    fn object(self) -> *mut Object {
        self.pointer as *mut Object
    }
    pub fn len(self) -> usize {
        unsafe { (*self.object()).len }
    }
    pub fn is_empty(self) -> bool {
        self.len() == 0
    }
    pub fn epoch(self) -> u64 {
        unsafe { (*self.object()).epoch }
    }
    unsafe fn entry_ptr(self, index: usize) -> *mut Entry<K, V> {
        unsafe { (((*self.object()).entries + 16) as *mut Entry<K, V>).add(index) }
    }
    pub fn entry(self, index: usize) -> Option<(K, V)> {
        if index >= self.len() {
            return None;
        }
        unsafe {
            let entry = self.entry_ptr(index);
            Some(((*entry).key, (*entry).value))
        }
    }
    fn bucket_len(self) -> usize {
        unsafe { (*self.object()).capacity * 2 }
    }
    unsafe fn bucket(self, index: usize) -> *mut usize {
        unsafe { (((*self.object()).buckets + 16) as *mut usize).add(index) }
    }
    unsafe fn index_entry(self, index: usize) {
        unsafe {
            let mask = self.bucket_len() - 1;
            let mut bucket = (*self.entry_ptr(index)).hash as usize & mask;
            while *self.bucket(bucket) != 0 {
                bucket = (bucket + 1) & mask;
            }
            self.bucket(bucket).write(index + 1);
        }
    }
    fn reserve(self) -> R<()> {
        let obj = self.object();
        let (len, capacity, desc) = unsafe { ((*obj).len, (*obj).capacity, (*obj).header.desc) };
        if len < capacity {
            return Ok(());
        }
        let capacity = capacity
            .checked_mul(2)
            .map(|v| v.max(4))
            .ok_or_else(|| allocation_failure(usize::MAX))?;
        let bytes = capacity
            .checked_mul(size_of::<Entry<K, V>>())
            .and_then(|v| v.checked_add(16))
            .ok_or_else(|| allocation_failure(usize::MAX))?;
        let buffer = unsafe { crate::try_alloc(desc + 1, bytes) }?;
        unsafe {
            // Zeroed spare entries trace safely. No allocation during copying.
            buffer.add(8).cast::<usize>().write(capacity);
            for i in 0..len {
                let old = self.entry_ptr(i);
                let new = buffer.add(16).cast::<Entry<K, V>>().add(i);
                K::store(buffer, &raw mut (*new).key, (*old).key);
                V::store(buffer, &raw mut (*new).value, (*old).value);
                (*new).hash = (*old).hash;
            }
            write_ptr(obj.cast(), &raw mut (*obj).entries, buffer as usize);
        }
        let bytes = capacity
            .checked_mul(16)
            .and_then(|v| v.checked_add(16))
            .ok_or_else(|| allocation_failure(usize::MAX))?;
        let buckets = unsafe { crate::try_alloc(desc + 3, bytes) }?;
        unsafe {
            buckets.add(8).cast::<usize>().write(capacity * 2);
            write_ptr(obj.cast(), &raw mut (*obj).buckets, buckets as usize);
            (*obj).capacity = capacity;
            for i in 0..len {
                self.index_entry(i);
            }
        }
        Ok(())
    }
    // A callback can mutate the map, grow its buffer, collect, or yield. Keep
    // only copied keys across it; restart the bucket walk if its epoch changes.
    fn find(
        self,
        cx: &mut Cx,
        key: K,
        hash: u64,
        equals: &mut impl FnMut(&mut Cx, K, K) -> R<bool>,
    ) -> R<Option<usize>> {
        'restart: loop {
            if self.is_empty() {
                return Ok(None);
            }
            let epoch = self.epoch();
            let mask = self.bucket_len() - 1;
            let mut bucket = hash as usize & mask;
            loop {
                let raw = unsafe { *self.bucket(bucket) };
                if raw == 0 {
                    return Ok(None);
                }
                let index = raw - 1;
                let entry = unsafe { self.entry_ptr(index) };
                if unsafe { (*entry).hash } == hash {
                    let candidate = unsafe { (*entry).key };
                    let equal = equals(cx, candidate, key)?;
                    if self.epoch() != epoch {
                        continue 'restart;
                    }
                    if equal {
                        return Ok(Some(index));
                    }
                }
                bucket = (bucket + 1) & mask;
            }
        }
    }
    pub fn get_with(
        self,
        cx: &mut Cx,
        key: K,
        hash: u64,
        mut equals: impl FnMut(&mut Cx, K, K) -> R<bool>,
    ) -> R<Option<V>> {
        Ok(self
            .find(cx, key, hash, &mut equals)?
            .map(|i| unsafe { (*self.entry_ptr(i)).value }))
    }
    pub fn set_with(
        self,
        cx: &mut Cx,
        key: K,
        value: V,
        hash: u64,
        mut equals: impl FnMut(&mut Cx, K, K) -> R<bool>,
    ) -> R<Option<V>> {
        if let Some(i) = self.find(cx, key, hash, &mut equals)? {
            unsafe {
                let entry = self.entry_ptr(i);
                let old = (*entry).value;
                V::store(
                    (*self.object()).entries as *mut u8,
                    &raw mut (*entry).value,
                    value,
                );
                (*self.object()).epoch = (*self.object()).epoch.wrapping_add(1);
                return Ok(Some(old));
            }
        }
        self.insert_new(key, value, hash)?;
        Ok(None)
    }
    fn insert_new(self, key: K, value: V, hash: u64) -> R<()> {
        self.reserve()?;
        unsafe {
            let obj = self.object();
            let i = (*obj).len;
            let entry = self.entry_ptr(i);
            K::store((*obj).entries as *mut u8, &raw mut (*entry).key, key);
            V::store((*obj).entries as *mut u8, &raw mut (*entry).value, value);
            (*entry).hash = hash;
            (*obj).len = i + 1;
            (*obj).epoch = (*obj).epoch.wrapping_add(1);
            self.index_entry(i);
        }
        Ok(())
    }
    pub fn get_or_insert_with(
        self,
        cx: &mut Cx,
        key: K,
        value: V,
        hash: u64,
        mut equals: impl FnMut(&mut Cx, K, K) -> R<bool>,
    ) -> R<V> {
        if let Some(i) = self.find(cx, key, hash, &mut equals)? {
            return Ok(unsafe { (*self.entry_ptr(i)).value });
        }
        self.insert_new(key, value, hash)?;
        Ok(value)
    }
    pub fn has_with(
        self,
        cx: &mut Cx,
        key: K,
        hash: u64,
        equals: impl FnMut(&mut Cx, K, K) -> R<bool>,
    ) -> R<bool> {
        Ok(self.get_with(cx, key, hash, equals)?.is_some())
    }
    pub fn snapshot(self, _cx: &mut Cx) -> R<Snapshot<K, V>> {
        let len = self.len();
        let bytes = len
            .checked_mul(size_of::<Entry<K, V>>())
            .and_then(|n| n.checked_add(16))
            .ok_or_else(|| allocation_failure(usize::MAX))?;
        let desc = unsafe { (*self.object()).header.desc };
        let buffer = unsafe { crate::try_alloc(desc + 1, bytes) }?;
        unsafe {
            buffer.add(8).cast::<usize>().write(len);
            for i in 0..len {
                let old = self.entry_ptr(i);
                let new = buffer.add(16).cast::<Entry<K, V>>().add(i);
                K::store(buffer, &raw mut (*new).key, (*old).key);
                V::store(buffer, &raw mut (*new).value, (*old).value);
                (*new).hash = (*old).hash;
            }
        }
        Ok(Snapshot {
            pointer: buffer as usize,
            owner: PhantomData,
        })
    }
    pub fn delete_with(
        self,
        cx: &mut Cx,
        key: K,
        hash: u64,
        mut equals: impl FnMut(&mut Cx, K, K) -> R<bool>,
    ) -> R<Option<V>> {
        let Some(i) = self.find(cx, key, hash, &mut equals)? else {
            return Ok(None);
        };
        unsafe {
            let obj = self.object();
            let old = (*self.entry_ptr(i)).value;
            let len = (*obj).len;
            for j in i..len - 1 {
                let source = self.entry_ptr(j + 1).read();
                let target = self.entry_ptr(j);
                K::store(
                    (*obj).entries as *mut u8,
                    &raw mut (*target).key,
                    source.key,
                );
                V::store(
                    (*obj).entries as *mut u8,
                    &raw mut (*target).value,
                    source.value,
                );
                (*target).hash = source.hash;
            }
            self.entry_ptr(len - 1).write_bytes(0, 1);
            (*obj).len = len - 1;
            (*obj).epoch = (*obj).epoch.wrapping_add(1);
            self.bucket(0).write_bytes(0, self.bucket_len());
            for j in 0..len - 1 {
                self.index_entry(j);
            }
            Ok(Some(old))
        }
    }
    pub fn clear(self) {
        unsafe {
            let obj = self.object();
            if (*obj).capacity != 0 {
                self.entry_ptr(0).write_bytes(0, (*obj).len);
                self.bucket(0).write_bytes(0, self.bucket_len());
            }
            (*obj).len = 0;
            (*obj).epoch = (*obj).epoch.wrapping_add(1);
        }
    }
}
impl<K: Key, V: Slot> Map<K, V> {
    pub fn get(self, cx: &mut Cx, key: K) -> R<Option<V>> {
        let hash = key.hash(cx);
        self.get_with(cx, key, hash, |cx, a, b| Ok(a.equals(b, cx)))
    }
    pub fn has(self, cx: &mut Cx, key: K) -> R<bool> {
        Ok(self.get(cx, key)?.is_some())
    }
    pub fn set(self, cx: &mut Cx, key: K, value: V) -> R<Option<V>> {
        let hash = key.hash(cx);
        self.set_with(cx, key, value, hash, |cx, a, b| Ok(a.equals(b, cx)))
    }
    pub fn delete(self, cx: &mut Cx, key: K) -> R<Option<V>> {
        let hash = key.hash(cx);
        self.delete_with(cx, key, hash, |cx, a, b| Ok(a.equals(b, cx)))
    }
    pub fn get_or_insert(self, cx: &mut Cx, key: K, value: V) -> R<V> {
        let hash = key.hash(cx);
        self.get_or_insert_with(cx, key, value, hash, |cx, a, b| Ok(a.equals(b, cx)))
    }
}
/// Immutable, managed snapshot: keeping it on the stack roots every entry,
/// including pointer/tagged values. Later mutation of the map is independent.
pub struct Snapshot<K: Slot, V: Slot> {
    pointer: usize,
    owner: PhantomData<(K, V, Rc<()>)>,
}
impl<K: Slot, V: Slot> Snapshot<K, V> {
    pub fn len(&self) -> usize {
        unsafe { *((self.pointer + 8) as *const usize) }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn entry(&self, index: usize) -> Option<(K, V)> {
        if index >= self.len() {
            return None;
        }
        let entry = unsafe { ((self.pointer + 16) as *const Entry<K, V>).add(index) };
        Some(unsafe { ((*entry).key, (*entry).value) })
    }
}
/// Compatibility adapter for packs generated before generic maps were added.
#[derive(Clone, Copy)]
pub struct StringIntMap(Map<Str, i64>);
impl StringIntMap {
    /// # Safety
    /// Same contract as Map::new.
    pub unsafe fn new(cx: &mut Cx, desc: u32) -> R<Self> {
        unsafe { Map::new(cx, desc).map(Self) }
    }
    /// # Safety
    /// Same contract as Map::from_raw.
    pub unsafe fn from_raw(p: usize) -> Self {
        Self(unsafe { Map::from_raw(p) })
    }
    pub fn as_raw(self) -> usize {
        self.0.as_raw()
    }
    pub(crate) fn structural_string(self, cx: &Cx) -> String {
        use std::fmt::Write;
        let mut text = String::from("{");
        for i in 0..self.0.len() {
            if i != 0 {
                text.push_str(", ");
            }
            let (key, value) = self.0.entry(i).unwrap();
            write!(&mut text, "{:?}: {}", key.text(cx), value).unwrap();
        }
        text.push('}');
        text
    }
    /// # Safety
    /// `desc` and `desc+1` describe a growing integer list and its buffer.
    pub unsafe fn growing_values(self, cx: &mut Cx, desc: u32) -> R<usize> {
        let list = unsafe { crate::lists::List::<i64>::with_capacity(cx, desc, self.0.len()) }?;
        for i in 0..self.0.len() {
            list.push(cx, self.0.entry(i).unwrap().1)?;
        }
        Ok(list.as_raw())
    }

    pub fn set(self, cx: &mut Cx, key: Str, value: i64) -> R<Dyn> {
        Ok(self
            .0
            .set(cx, key, value)?
            .map_or(Dyn { tag: 2, payload: 0 }, |v| Dyn {
                tag: 0,
                payload: v as u64,
            }))
    }
    /// # Safety
    /// `desc` describes a growing integer list and its buffer.
    pub unsafe fn values(self, cx: &mut Cx, desc: u32) -> R<usize> {
        unsafe { self.growing_values(cx, desc) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Instance, language::*};
    const DYN: [Descriptor; 4] = typed_descriptors::<Str, Dyn>(11);
    const PTR: [Descriptor; 4] = typed_descriptors::<i64, Str>(15);
    const FLOAT: [Descriptor; 4] = typed_descriptors::<f64, i64>(19);
    static DESCRIPTORS: &[Descriptor] = &[
        STRING_DESC,
        OVERFLOW_DESC,
        SHIFT_DESC,
        CANCEL_DESC,
        STACK_DESC,
        ALLOC_DESC,
        INDEX_DESC,
        crate::json::PARSE_DESC,
        crate::json::DECODE_DESC,
        DIV_DESC,
        MAP_KEY_DESC,
        DYN[0],
        DYN[1],
        DYN[2],
        DYN[3],
        PTR[0],
        PTR[1],
        PTR[2],
        PTR[3],
        FLOAT[0],
        FLOAT[1],
        FLOAT[2],
        FLOAT[3],
    ];
    const fn pointer_types() -> [bool; 26] {
        let mut types = [true; 26];
        types[0] = false;
        types[1] = false;
        types[2] = false;
        types
    }
    static POINTERS: [bool; 26] = pointer_types();
    fn instance() -> Instance {
        let mut instance = Instance::new(DESCRIPTORS, &POINTERS);
        instance.set_stress(true);
        instance
    }
    #[test]
    fn float_keys_use_reflexive_nan_equality_and_signed_zero() {
        fn run() -> R<i64> {
            let mut cx = unsafe { Cx::current() };
            let map = unsafe { Map::<f64, i64>::new(&mut cx, 19) }?;
            map.set(&mut cx, f64::NAN, 7)?;
            let different_nan = f64::from_bits(0x7ff0000000000001);
            assert_eq!(map.set(&mut cx, different_nan, 9)?, Some(7));
            map.set(&mut cx, -0.0, 11)?;
            assert_eq!(map.get(&mut cx, 0.0)?, Some(11));
            assert_eq!(map.len(), 2);
            Ok(2)
        }
        assert_eq!(unsafe { instance().run_baml(run) }.unwrap(), 2);
    }
    #[test]
    fn ordered_generic_values_mutations_and_gc() {
        fn run() -> R<i64> {
            let mut cx = unsafe { Cx::current() };
            let map = unsafe { Map::<Str, Dyn>::new(&mut cx, 11) }?;
            for i in 0..96 {
                let key = cx.string(&format!("key-{i}"))?;
                let value = cx.string(&format!("value-{i}"))?;
                assert!(
                    map.set(
                        &mut cx,
                        key,
                        Dyn {
                            tag: 3,
                            payload: value.as_raw() as u64
                        }
                    )?
                    .is_none()
                );
                cx.poll()?;
            }
            let key = cx.string("key-3")?;
            let old = map.set(&mut cx, key, Dyn { tag: 2, payload: 0 })?.unwrap();
            assert_eq!(
                unsafe { Str::from_raw(old.payload as usize) }.as_str(&cx),
                "value-3"
            );
            assert!(map.has(&mut cx, key)?); // A stored null remains present.
            let before = map.entry(3).unwrap();
            assert_eq!(before.0.as_str(&cx), "key-3");
            assert_eq!(map.delete(&mut cx, key)?.unwrap().tag, 2);
            assert!(!map.has(&mut cx, key)?);
            let replacement = cx.string("replacement")?;
            map.set(
                &mut cx,
                key,
                Dyn {
                    tag: 3,
                    payload: replacement.as_raw() as u64,
                },
            )?;
            cx.poll()?;
            assert_eq!(map.entry(3).unwrap().0.as_str(&cx), "key-4");
            assert_eq!(map.entry(95).unwrap().0.as_str(&cx), "key-3");
            for i in 0..map.len() {
                let (key, value) = map.entry(i).unwrap();
                assert!(key.as_str(&cx).starts_with("key-"));
                assert!(
                    unsafe { Str::from_raw(value.payload as usize) }
                        .as_str(&cx)
                        .starts_with(if i == 95 { "replacement" } else { "value-" })
                );
            }
            let snapshot = map.snapshot(&mut cx)?;
            map.clear();
            assert!(map.is_empty());
            cx.poll()?;
            for i in 0..snapshot.len() {
                let (key, value) = snapshot.entry(i).unwrap();
                assert!(key.as_str(&cx).starts_with("key-"));
                assert!(
                    unsafe { Str::from_raw(value.payload as usize) }
                        .as_str(&cx)
                        .starts_with(if i == 95 { "replacement" } else { "value-" })
                );
            }
            Ok(96)
        }
        assert_eq!(unsafe { instance().run_baml(run) }.unwrap(), 96);
    }
    #[test]
    fn json_order_duplicates_and_error_paths_under_stress() {
        fn run() -> R<i64> {
            let mut cx = unsafe { Cx::current() };
            let text = cx.string(r#"{"z":"discarded","a":2,"z":3}"#)?;
            let map = unsafe {
                json::decode::<Dyn>(&mut cx, 11, text, |cx, reader, path| {
                    json::integer(cx, reader, path).map(|v| Dyn {
                        tag: 0,
                        payload: v as u64,
                    })
                })
            }?;
            cx.poll()?;
            assert_eq!(map.entry(0).unwrap().0.as_str(&cx), "z");
            let text = json::stringify(&mut cx, map, |cx, v, _, out| {
                map.clear();
                cx.poll()?;
                out.push_str(&(v.payload as i64).to_string());
                Ok(())
            })?;
            assert_eq!(text.as_str(&cx), r#"{"z":3,"a":2}"#);
            let text = cx.string(r#"{"a":false}"#)?;
            let error = unsafe {
                json::decode::<Dyn>(&mut cx, 11, text, |cx, reader, path| {
                    json::integer(cx, reader, path).map(|v| Dyn {
                        tag: 0,
                        payload: v as u64,
                    })
                })
            }
            .err()
            .unwrap();
            assert_eq!(error.class, "baml.json.DecodeError");
            assert_eq!(error.message, "expected integer");
            let text = cx.string(r#"{"a":false,}"#)?;
            let error = unsafe {
                json::decode::<Dyn>(&mut cx, 11, text, |cx, reader, path| {
                    json::integer(cx, reader, path).map(|v| Dyn {
                        tag: 0,
                        payload: v as u64,
                    })
                })
            }
            .err()
            .unwrap();
            assert_eq!(error.class, "baml.json.ParseError");
            Ok(1)
        }
        assert_eq!(unsafe { instance().run_baml(run) }.unwrap(), 1);
    }
    #[test]
    fn equality_callback_restarts_after_mutation_and_collection() {
        fn run() -> R<i64> {
            let mut cx = unsafe { Cx::current() };
            let map = unsafe { Map::<i64, Str>::new(&mut cx, 15) }?;
            let old = cx.string("old")?;
            map.set_with(&mut cx, 1, old, 0, |_, a, b| Ok(a == b))?;
            let mut mutated = false;
            let found = map
                .get_with(&mut cx, 1, 0, |cx, a, b| {
                    if !mutated {
                        mutated = true;
                        map.clear();
                        let new = cx.string("new")?;
                        map.set_with(cx, 1, new, 0, |_, a, b| Ok(a == b))?;
                        cx.poll()?;
                    }
                    Ok(a == b)
                })?
                .unwrap();
            assert_eq!(found.as_str(&cx), "new");
            assert!(mutated);
            Ok(1)
        }
        assert_eq!(unsafe { instance().run_baml(run) }.unwrap(), 1);
    }
}

impl<V: Slot> Map<Str, V> {
    /// Heap-neutral lookup for JSON field/path readers. No temporary managed key.
    pub fn get_text(self, cx: &Cx, key: &str) -> Option<V> {
        if self.is_empty() {
            return None;
        }
        let hash = slots::hash_bytes(key.as_bytes());
        let mask = self.bucket_len() - 1;
        let mut bucket = hash as usize & mask;
        loop {
            let raw = unsafe { *self.bucket(bucket) };
            if raw == 0 {
                return None;
            }
            let entry = unsafe { &*self.entry_ptr(raw - 1) };
            if entry.hash == hash && entry.key.text(cx) == key {
                return Some(entry.value);
            }
            bucket = (bucket + 1) & mask;
        }
    }
}
