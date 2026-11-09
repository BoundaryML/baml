//! Owned native-provider boundary. No managed address crosses an await.
use crate::{Dyn, Error, R};
use baml_rt_gc::SlotKind;
pub use bex_external_types::{BexExternalValue as Owned, WireTy};
type Resource = std::sync::Arc<dyn bex_vm_types::BexRustData>;

pub struct Type {
    pub tag: u64,
    pub wire: fn() -> WireTy,
    pub shape: Shape,
}
pub enum Shape {
    Null,
    Int,
    Float,
    Bool,
    String,
    Bigint,
    Bytes,
    Opaque,
    Optional(&'static Type),
    Union(&'static [&'static Type]),
    Class(&'static Class),
    Enum {
        name: &'static str,
        variants: &'static [&'static str],
    },
    List {
        desc: u32,
        growing: bool,
        element: &'static Type,
        slot: SlotKind,
        stride: usize,
    },
    Map {
        encode: unsafe fn(usize, usize) -> R<Owned>,
        hydrate: unsafe fn(&Owned, usize) -> R<usize>,
    },
}
pub struct Class {
    pub name: &'static str,
    pub desc: u32,
    pub bytes: usize,
    pub fields: &'static [Field],
}
pub struct Field {
    pub name: &'static str,
    pub offset: usize,
    pub slot: SlotKind,
    pub ty: &'static Type,
}
fn depth(n: usize) -> R<()> {
    if n > 512 {
        Err(Error::internal("owned value nesting exceeds 512".into()))
    } else {
        Ok(())
    }
}
fn invalid() -> Box<Error> {
    Error::internal("native provider value does not match its declared type".into())
}

impl Type {
    pub fn accepts(&self, v: &Owned) -> bool {
        match (&self.shape, v) {
            (Shape::Null, Owned::Null)
            | (Shape::Int, Owned::Int(_))
            | (Shape::Float, Owned::Float(_))
            | (Shape::Bool, Owned::Bool(_))
            | (Shape::String, Owned::String(_))
            | (Shape::Bigint, Owned::Bigint(_))
            | (Shape::Bytes, Owned::Uint8Array(_))
            | (Shape::Opaque, Owned::RustData(_))
            | (Shape::List { .. }, Owned::Array { .. })
            | (Shape::Map { .. }, Owned::Map { .. }) => true,
            (Shape::Class(c), Owned::Instance { class_name, .. }) => class_name == c.name,
            (Shape::Enum { name, .. }, Owned::Variant { enum_name, .. }) => enum_name == name,
            (Shape::Optional(inner), v) => matches!(v, Owned::Null) || inner.accepts(v),
            (Shape::Union(members), v) => members.iter().any(|t| t.accepts(v)),
            _ => false,
        }
    }
    /// # Safety
    /// value and every reachable object are live in the current owner instance.
    pub unsafe fn copy_out(&self, value: Dyn, n: usize) -> R<Owned> {
        depth(n)?;
        let p = value.payload as usize;
        Ok(match &self.shape {
            Shape::Null => Owned::Null,
            Shape::Int => Owned::Int(value.payload as i64),
            Shape::Float => Owned::Float(f64::from_bits(value.payload)),
            Shape::Bool => Owned::Bool(value.payload != 0),
            Shape::String => Owned::String(
                unsafe { crate::strings::Str::from_raw(p) }
                    .try_as_str(&unsafe { crate::strings::Cx::current() })?
                    .to_owned()
                    .into(),
            ),
            Shape::Bigint => Owned::Bigint(unsafe { crate::native::bigint_owned(p) }),
            Shape::Bytes => Owned::Uint8Array(unsafe { crate::native::bytes_owned(p) }),
            Shape::Opaque => Owned::RustData(
                unsafe { crate::opaque::Opaque::<Resource>::from_raw(p) }.with(Clone::clone),
            ),
            Shape::Optional(inner) => {
                if value.tag == 2 {
                    Owned::Null
                } else {
                    unsafe { inner.copy_out(value, n)? }
                }
            }
            Shape::Union(types) => {
                let t = types
                    .iter()
                    .find(|t| t.tag == value.tag)
                    .ok_or_else(invalid)?;
                unsafe { t.copy_out(value, n)? }
            }
            Shape::Class(c) => {
                let mut fields = indexmap::IndexMap::new();
                for f in c.fields {
                    let v = unsafe { f.ty.read((p + f.offset) as *const u8, f.slot) };
                    fields.insert(f.name.into(), unsafe { f.ty.copy_out(v, n + 1)? });
                }
                Owned::Instance {
                    class_name: c.name.into(),
                    type_args: Vec::new(),
                    fields,
                }
            }
            Shape::Enum { name, variants } => Owned::Variant {
                enum_name: (*name).into(),
                variant_name: variants.get(p).ok_or_else(invalid)?.to_string(),
            },
            Shape::List {
                element,
                growing,
                slot,
                stride,
                ..
            } => {
                let object = p as *const u8;
                let len = unsafe {
                    object
                        .add(if *growing { 16 } else { 8 })
                        .cast::<usize>()
                        .read()
                };
                let buffer = if *growing {
                    unsafe { object.add(8).cast::<*const u8>().read() }
                } else {
                    object
                };
                let mut items = Vec::with_capacity(len);
                for i in 0..len {
                    let v = unsafe { element.read(buffer.add(16 + i * stride), *slot) };
                    items.push(unsafe { element.copy_out(v, n + 1)? });
                }
                Owned::Array {
                    element_type: (element.wire)(),
                    items,
                }
            }
            Shape::Map { encode, .. } => unsafe { encode(p, n + 1)? },
        })
    }
    /// # Safety
    /// Called only on the instance owner. Incoming data owns no managed pointers.
    pub unsafe fn hydrate(&self, value: &Owned, n: usize) -> R<Dyn> {
        depth(n)?;
        if let Owned::Union { value, .. } = value {
            return unsafe { self.hydrate(value, n) };
        }
        if let Shape::Optional(inner) = &self.shape {
            return if matches!(value, Owned::Null) {
                Ok(Dyn { tag: 2, payload: 0 })
            } else {
                unsafe { inner.hydrate(value, n) }
            };
        }
        if let Shape::Union(types) = &self.shape {
            let ty = types
                .iter()
                .find(|t| t.accepts(value))
                .ok_or_else(invalid)?;
            return unsafe { ty.hydrate(value, n) };
        }
        let payload = match (&self.shape, value) {
            (Shape::Null, Owned::Null) => 0,
            (Shape::Int, Owned::Int(v)) if baml_type::Int63::new(*v).is_some() => *v as u64,
            (Shape::Float, Owned::Float(v)) => v.to_bits(),
            (Shape::Bool, Owned::Bool(v)) => u64::from(*v),
            (Shape::String, Owned::String(v)) => unsafe { crate::strings::Cx::current() }
                .string(v.as_str())?
                .as_raw() as u64,
            (Shape::Bigint, Owned::Bigint(v)) => crate::native::bigint(v.clone())? as u64,
            (Shape::Bytes, Owned::Uint8Array(v)) => crate::native::bytes(v.clone())? as u64,
            (Shape::Opaque, Owned::RustData(v)) => {
                crate::opaque::Opaque::new(15, v.clone())?.as_raw() as u64
            }
            (
                Shape::Class(c),
                Owned::Instance {
                    class_name, fields, ..
                },
            ) if class_name == c.name => {
                let p = unsafe { crate::try_alloc(c.desc, c.bytes)? };
                for field in c.fields {
                    let v = fields.get(field.name).unwrap_or(&Owned::Null);
                    let v = unsafe { field.ty.hydrate(v, n + 1)? };
                    unsafe { write(p, p.add(field.offset), field.slot, v) };
                }
                p as u64
            }
            (
                Shape::Enum { name, variants },
                Owned::Variant {
                    enum_name,
                    variant_name,
                },
            ) if enum_name == name => variants
                .iter()
                .position(|v| *v == variant_name)
                .ok_or_else(invalid)? as u64,
            (
                Shape::List {
                    desc,
                    growing,
                    element,
                    slot,
                    stride,
                },
                Owned::Array { items, .. },
            ) => {
                let bytes = items
                    .len()
                    .checked_mul(*stride)
                    .and_then(|v| v.checked_add(16))
                    .ok_or_else(|| crate::allocation_failure(usize::MAX))?;
                let p = unsafe { crate::try_alloc(*desc, if *growing { 32 } else { bytes })? };
                let buffer = if *growing {
                    let buffer = unsafe { crate::try_alloc(desc + 1, bytes)? };
                    unsafe {
                        baml_rt_gc::write_ptr(p, p.add(8).cast(), buffer as usize);
                        p.add(16).cast::<usize>().write(items.len());
                        p.add(24).cast::<usize>().write(items.len());
                    }
                    buffer
                } else {
                    p
                };
                unsafe { buffer.add(8).cast::<usize>().write(items.len()) };
                for (i, item) in items.iter().enumerate() {
                    let v = unsafe { element.hydrate(item, n + 1)? };
                    unsafe { write(buffer, buffer.add(16 + i * stride), *slot, v) };
                }
                p as u64
            }
            (Shape::Map { hydrate, .. }, Owned::Map { .. }) => unsafe {
                hydrate(value, n + 1)? as u64
            },
            _ => return Err(invalid()),
        };
        Ok(Dyn {
            tag: self.tag,
            payload,
        })
    }
    /// # Safety
    /// slot uses the supplied ABI and belongs to a live managed object.
    pub unsafe fn read(&self, slot: *const u8, kind: SlotKind) -> Dyn {
        if matches!(kind, SlotKind::Tagged) {
            return unsafe { slot.cast::<Dyn>().read() };
        };
        let payload = unsafe { slot.cast::<u64>().read() };
        if let Shape::Optional(inner) = &self.shape {
            return if payload == 0 {
                Dyn { tag: 2, payload: 0 }
            } else {
                unsafe { inner.read(slot, kind) }
            };
        };
        Dyn {
            tag: self.tag,
            payload,
        }
    }
}
unsafe fn write(owner: *mut u8, slot: *mut u8, kind: SlotKind, value: Dyn) {
    match kind {
        SlotKind::Pointer => unsafe {
            baml_rt_gc::write_ptr(owner, slot.cast(), value.payload as usize)
        },
        SlotKind::Tagged => unsafe { crate::write_dyn(owner, slot.cast(), value) },
        SlotKind::Scalar => unsafe { slot.cast::<u64>().write(value.payload) },
    }
}

pub trait SlotValue: crate::maps::Slot {
    fn dynamic(self, tag: u64) -> Dyn;
    unsafe fn from_dynamic(value: Dyn) -> Self;
}
impl SlotValue for i64 {
    fn dynamic(self, tag: u64) -> Dyn {
        Dyn {
            tag,
            payload: self as u64,
        }
    }
    unsafe fn from_dynamic(v: Dyn) -> Self {
        v.payload as i64
    }
}
impl SlotValue for f64 {
    fn dynamic(self, tag: u64) -> Dyn {
        Dyn {
            tag,
            payload: self.to_bits(),
        }
    }
    unsafe fn from_dynamic(v: Dyn) -> Self {
        f64::from_bits(v.payload)
    }
}
impl SlotValue for bool {
    fn dynamic(self, tag: u64) -> Dyn {
        Dyn {
            tag,
            payload: u64::from(self),
        }
    }
    unsafe fn from_dynamic(v: Dyn) -> Self {
        v.payload != 0
    }
}
impl SlotValue for Dyn {
    fn dynamic(self, _tag: u64) -> Dyn {
        self
    }
    unsafe fn from_dynamic(v: Dyn) -> Self {
        v
    }
}
impl SlotValue for crate::strings::Str {
    fn dynamic(self, tag: u64) -> Dyn {
        Dyn {
            tag: if self.as_raw() == 0 { 2 } else { tag },
            payload: self.as_raw() as u64,
        }
    }
    unsafe fn from_dynamic(v: Dyn) -> Self {
        unsafe { Self::from_raw(v.payload as usize) }
    }
}
impl<const TAG: u64> SlotValue for crate::maps::Pointer<TAG> {
    fn dynamic(self, _tag: u64) -> Dyn {
        Dyn {
            tag: if self.as_raw() == 0 { 2 } else { TAG },
            payload: self.as_raw() as u64,
        }
    }
    unsafe fn from_dynamic(v: Dyn) -> Self {
        unsafe { Self::from_raw(v.payload as usize) }
    }
}
impl SlotValue for crate::maps::Null {
    fn dynamic(self, _tag: u64) -> Dyn {
        Dyn { tag: 2, payload: 0 }
    }
    unsafe fn from_dynamic(_v: Dyn) -> Self {
        Self
    }
}
/// # Safety
/// map has the given string-key/value storage ABI in the current instance.
pub unsafe fn copy_string_map<V: SlotValue>(
    map: usize,
    value_type: &'static Type,
    n: usize,
) -> R<Owned> {
    depth(n)?;
    let map = unsafe { crate::maps::Map::<crate::strings::Str, V>::from_raw(map) };
    let mut entries = indexmap::IndexMap::new();
    // Copies stay on the owner thread, and the source map stays live while its
    // reachable objects are copied. No BAML callback or suspension occurs here.
    for (key, value) in map.copied_entries() {
        let key = key
            .try_as_str(&unsafe { crate::strings::Cx::current() })?
            .to_owned();
        let value = unsafe { value_type.copy_out(value.dynamic(value_type.tag), n + 1)? };
        entries.insert(key, value);
    }
    Ok(Owned::Map {
        key_type: WireTy::String,
        value_type: (value_type.wire)(),
        entries,
    })
}
/// # Safety
/// desc describes a string-key map with V slots in the current instance.
pub unsafe fn hydrate_string_map<V: SlotValue>(
    owned: &Owned,
    value_type: &'static Type,
    desc: u32,
    n: usize,
) -> R<usize> {
    depth(n)?;
    let Owned::Map { entries, .. } = owned else {
        return Err(invalid());
    };
    let mut cx = unsafe { crate::strings::Cx::current() };
    let map = unsafe { crate::maps::Map::<crate::strings::Str, V>::new(&mut cx, desc)? };
    for (key, value) in entries {
        let key = cx.string(key)?;
        let value = unsafe { V::from_dynamic(value_type.hydrate(value, n + 1)?) };
        map.set(&mut cx, key, value)?;
    }
    Ok(map.as_raw())
}

#[cfg(test)]
mod tests {
    use super::*;
    use baml_rt_gc::{Descriptor, Field as GcField, Kind};
    struct Resource;
    impl bex_vm_types::BexRustData for Resource {
        fn measure(&self, _: &mut bex_vm_types::Meter) {}
    }
    static INT: Type = Type {
        tag: 0,
        wire: || WireTy::Int,
        shape: Shape::Int,
    };
    static TEXT: Type = Type {
        tag: 3,
        wire: || WireTy::String,
        shape: Shape::String,
    };
    static BIG: Type = Type {
        tag: crate::native::BIGINT_TAG,
        wire: || WireTy::Bigint,
        shape: Shape::Bigint,
    };
    static BYTES: Type = Type {
        tag: 19,
        wire: || WireTy::Uint8Array,
        shape: Shape::Bytes,
    };
    static RESOURCE: Type = Type {
        tag: crate::native::OPAQUE_TAG,
        wire: || WireTy::Unknown,
        shape: Shape::Opaque,
    };
    static OPTIONAL: Type = Type {
        tag: u64::MAX,
        wire: || WireTy::Unknown,
        shape: Shape::Optional(&INT),
    };
    static MODE: Type = Type {
        tag: 100,
        wire: || WireTy::Unknown,
        shape: Shape::Enum {
            name: "Mode",
            variants: &["Cold", "Hot"],
        },
    };
    static FLOAT: Type = Type {
        tag: crate::scalars::FLOAT_TAG,
        wire: || WireTy::Float,
        shape: Shape::Float,
    };
    static BOX: Type = Type {
        tag: 23,
        wire: || WireTy::Unknown,
        shape: Shape::Class(&Class {
            name: "NativeBox",
            desc: 20,
            bytes: 72,
            fields: &[
                Field {
                    name: "text",
                    offset: 8,
                    slot: SlotKind::Pointer,
                    ty: &TEXT,
                },
                Field {
                    name: "big",
                    offset: 16,
                    slot: SlotKind::Pointer,
                    ty: &BIG,
                },
                Field {
                    name: "bytes",
                    offset: 24,
                    slot: SlotKind::Pointer,
                    ty: &BYTES,
                },
                Field {
                    name: "resource",
                    offset: 32,
                    slot: SlotKind::Pointer,
                    ty: &RESOURCE,
                },
                Field {
                    name: "maybe",
                    offset: 40,
                    slot: SlotKind::Tagged,
                    ty: &OPTIONAL,
                },
                Field {
                    name: "mode",
                    offset: 56,
                    slot: SlotKind::Scalar,
                    ty: &MODE,
                },
                Field {
                    name: "float",
                    offset: 64,
                    slot: SlotKind::Scalar,
                    ty: &FLOAT,
                },
            ],
        }),
    };
    const fn descriptors() -> [Descriptor; 21] {
        let mut d = [crate::STRING_DESC; 21];
        d[17] = crate::native::BIGINT_LAYOUT;
        d[16] = crate::native::BYTES_LAYOUT;
        d[15] = crate::opaque::descriptor(18);
        d[18] = crate::native::BYTES_BUFFER_LAYOUT;
        d[19] = crate::USER_PANIC_DESC;
        d[20] = Descriptor {
            kind: Kind::Class,
            size: 72,
            ptr_slots: (8, 4),
            tagged_slots: (40, 1),
            elem: None,
            type_id: 23,
            finalizer: None,
            fields: &[
                GcField {
                    name: "text",
                    offset: 8,
                    slot: SlotKind::Pointer,
                    type_id: 3,
                },
                GcField {
                    name: "big",
                    offset: 16,
                    slot: SlotKind::Pointer,
                    type_id: 18,
                },
                GcField {
                    name: "bytes",
                    offset: 24,
                    slot: SlotKind::Pointer,
                    type_id: 19,
                },
                GcField {
                    name: "resource",
                    offset: 32,
                    slot: SlotKind::Pointer,
                    type_id: 20,
                },
                GcField {
                    name: "maybe",
                    offset: 40,
                    slot: SlotKind::Tagged,
                    type_id: 0,
                },
            ],
        };
        d
    }
    static DESCRIPTORS: [Descriptor; 21] = descriptors();
    static TAGS: [bool; 24] = {
        let mut tags = [true; 24];
        tags[0] = false;
        tags[1] = false;
        tags[2] = false;
        tags
    };
    #[test]
    fn owned_resources_and_mixed_fields_survive_gc_stress() {
        let resource: std::sync::Arc<dyn bex_vm_types::BexRustData> = std::sync::Arc::new(Resource);
        let expected = Owned::Instance {
            class_name: "NativeBox".into(),
            type_args: Vec::new(),
            fields: indexmap::indexmap! {
                "text".into()=>Owned::String("copied".into()),
                "big".into()=>Owned::Bigint((num_bigint::BigInt::from(1)<<150usize)+7),
                "bytes".into()=>Owned::Uint8Array(vec![0,255,7]),
                "resource".into()=>Owned::RustData(resource),
                "maybe".into()=>Owned::Int(0),
                "mode".into()=>Owned::Variant{enum_name:"Mode".into(),variant_name:"Hot".into()},
                "float".into()=>Owned::Float(f64::INFINITY),
            },
        };
        let source = expected.clone();
        let mut instance = crate::Instance::new(&DESCRIPTORS, &TAGS);
        instance.set_stress(true);
        let actual = unsafe {
            instance.run_owned(move || -> R<Owned> {
                let value = BOX.hydrate(&source, 0)?;
                for _ in 0..32 {
                    let _ = crate::native::bytes(vec![7; 100])?;
                    crate::safepoint().unwrap();
                }
                BOX.copy_out(value, 0)
            })
        }
        .unwrap();
        assert_eq!(actual, expected);
        assert!(instance.stats().collections > 30);
        drop(instance);
        assert_eq!(
            actual, expected,
            "owned result must remain usable after instance destruction"
        );
    }
}
