//! Canonical captures of the admitted rt layouts. Traversal never allocates in
//! the managed heap, calls user code, or keeps a managed pointer after freezing.
// Layout fields are pointer-aligned by the compiler; all referenced objects
// have the heap's eight-byte header and aligned scalar/tagged slots.
#![allow(clippy::cast_ptr_alignment)]
use baml_rt_gc::TaggedSlot as Dyn;
use baml_rt_gc::{Field, SlotKind};
use btel_snapshot::{
    Builder, Carried, Leaves, Limit, Limits, OwnedType, Reserved, Shaper, Snapshot, SnapshotObject,
    SnapshotPool, SnapshotRoot, SnapshotValue, TypeIdentity,
};
use btel_types::Definition;
use std::collections::HashMap;

// Container runtime element types can differ from their static field types.
// Regex natives produce unknown elements; nullable class literals retain their
// concrete optional type even though they share the pointer storage layout.
pub const UNKNOWN_ELEMENT_TYPE: u32 = 1;
pub const OPTIONAL_ELEMENT_TYPE: u32 = 2;
fn element_type(ty: &Type, aux: u32, leaves: &mut Leaves<'_>, carried: &mut Carried) -> OwnedType {
    if aux == UNKNOWN_ELEMENT_TYPE {
        return OwnedType::Unknown;
    }
    let ty = ty.owned(leaves, carried);
    if aux == OPTIONAL_ELEMENT_TYPE {
        OwnedType::Union(vec![ty, OwnedType::Null].into())
    } else {
        ty
    }
}

pub struct Metadata {
    pub shapes: &'static [Shape],
    pub naming_tag: u64,
}

pub struct Declaration {
    pub name: baml_type::DeclarationName,
    pub tag: baml_type::typetag::TypeTag,
    pub definition: Definition,
    pub is_enum: bool,
}
pub enum Shape {
    Bigint,
    String,
    Uint8Array,
    RustType,
    Enum {
        declaration: fn() -> &'static Declaration,
        variants: &'static [&'static str],
    },
    Class {
        declaration: fn() -> &'static Declaration,
        fields: &'static [Field],
    },
    GenericClass {
        declaration: fn() -> &'static Declaration,
        fields: &'static [Field],
        args: &'static [Type],
    },
    List {
        growing: bool,
        tag: u64,
        element: &'static Type,
    },
    MapStringInt,
    Closure,
    Future,
    MapStringPtr {
        tag: u64,
        value: &'static Type,
    },
    Unsupported,
}
pub use baml_type::Freshness;
pub enum Type {
    Bigint,
    Future(&'static Type, &'static Type),
    Function {
        params: &'static [Parameter],
        ret: &'static Type,
        throws: &'static Type,
    },
    Int,
    LiteralInt(i64, Freshness),
    LiteralBool(bool, Freshness),
    LiteralString(&'static str, Freshness),
    LiteralFloat(&'static str, Freshness),
    Float,
    Null,
    Unknown,
    Never,
    Bool,
    String,
    Uint8Array,
    RustType,
    Class(fn() -> &'static Declaration),
    GenericClass(fn() -> &'static Declaration, &'static [Type]),
    Enum(fn() -> &'static Declaration),
    EnumVariant(fn() -> &'static Declaration, &'static str),
    Alias(i64, &'static [u8]),
    Interface(
        i64,
        &'static [u8],
        &'static [Type],
        &'static [(&'static str, Type)],
    ),
    Map(&'static Type, &'static Type),
    List(&'static Type),
    Union(&'static [Type]),
}
pub struct Parameter {
    pub name: Option<&'static str>,
    pub optional: bool,
    pub ty: &'static Type,
}
impl Type {
    fn owned(&self, leaves: &mut Leaves<'_>, carried: &mut Carried) -> OwnedType {
        match self {
            Self::Future(value, error) => OwnedType::Future(
                Box::new(value.owned(leaves, carried)),
                Box::new(error.owned(leaves, carried)),
            ),
            Self::Function {
                params,
                ret,
                throws,
            } => OwnedType::Function {
                params: params
                    .iter()
                    .map(|p| baml_type::RealizedFunctionParamTy {
                        name: p.name.map(baml_type::Name::new),
                        mode: if p.optional {
                            baml_type::FunctionParamMode::Optional
                        } else {
                            baml_type::FunctionParamMode::Required
                        },
                        ty: p.ty.owned(leaves, carried),
                    })
                    .collect(),
                ret: Box::new(ret.owned(leaves, carried)),
                throws: Box::new(throws.owned(leaves, carried)),
            },
            Self::Bigint => OwnedType::Bigint,
            Self::Int => OwnedType::Int,
            Self::LiteralInt(v, f) => OwnedType::Literal(baml_type::Literal::Int(*v), *f),
            Self::LiteralBool(v, f) => OwnedType::Literal(baml_type::Literal::Bool(*v), *f),
            Self::LiteralFloat(v, f) => {
                OwnedType::Literal(baml_type::Literal::Float((*v).into()), *f)
            }
            Self::LiteralString(v, f) => {
                OwnedType::Literal(baml_type::Literal::String((*v).into()), *f)
            }
            Self::Float => OwnedType::Float,
            Self::Null => OwnedType::Null,
            Self::Unknown => OwnedType::Unknown,
            Self::Never => OwnedType::Never,
            Self::Alias(tag, name) => OwnedType::TypeAlias(named_identity(*tag, name)),
            Self::Interface(tag, name, args, assoc) => OwnedType::Interface(
                named_identity(*tag, name),
                args.iter().map(|t| t.owned(leaves, carried)).collect(),
                assoc
                    .iter()
                    .map(|(n, t)| ((*n).into(), t.owned(leaves, carried)))
                    .collect(),
            ),
            Self::EnumVariant(declaration, variant) => OwnedType::EnumVariant(
                TypeIdentity::Defined(leaves.define(&declaration().definition, carried)),
                (*variant).into(),
            ),
            Self::Enum(declaration) => OwnedType::Enum(TypeIdentity::Defined(
                leaves.define(&declaration().definition, carried),
            )),
            Self::GenericClass(declaration, args) => OwnedType::Class(
                TypeIdentity::Defined(leaves.define(&declaration().definition, carried)),
                args.iter().map(|t| t.owned(leaves, carried)).collect(),
            ),
            Self::Map(k, v) => OwnedType::Map {
                key: Box::new(k.owned(leaves, carried)),
                value: Box::new(v.owned(leaves, carried)),
            },
            Self::Bool => OwnedType::Bool,
            Self::String => OwnedType::String,
            Self::Uint8Array => OwnedType::Uint8Array,
            Self::RustType => OwnedType::RustType,
            Self::Class(declaration) => OwnedType::Class(
                TypeIdentity::Defined(leaves.define(&declaration().definition, carried)),
                Box::new([]),
            ),
            Self::List(element) => OwnedType::List(Box::new(element.owned(leaves, carried))),
            Self::Union(members) => OwnedType::Union(
                members
                    .iter()
                    .map(|member| member.owned(leaves, carried))
                    .collect(),
            ),
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Key {
    Value(usize),
    Declaration(u64),
    Enum(u64, u64),
}
struct Work {
    slot: Reserved,
    key: Key,
    tag: u64,
    depth: usize,
}
pub struct Scratch {
    seen: HashMap<Key, SnapshotValue>,
    work: Vec<Work>,
    carried: Carried,
    shaper: Shaper,
    pool: SnapshotPool,
}
impl Default for Scratch {
    fn default() -> Self {
        Self {
            seen: HashMap::new(),
            work: Vec::new(),
            carried: Carried::default(),
            shaper: Shaper::default(),
            pool: SnapshotPool::unbounded(Limits::default()),
        }
    }
}
impl Scratch {
    /// # Safety
    /// Every pointer belongs to the current heap and is live throughout this call.
    pub unsafe fn arguments(&mut self, shapes: &[Shape], values: &[Dyn]) -> Snapshot {
        self.begin();
        let mut b = self.pool.acquire_beyond_budget();
        let root = b.arguments(values.iter(), |leaves, value| unsafe {
            self.add(shapes, leaves, *value, 0)
        });
        unsafe { self.finish(shapes, b, root) }
    }
    /// # Safety
    /// Same pointer lifetime requirements as `arguments`.
    pub unsafe fn value(&mut self, shapes: &[Shape], value: Dyn) -> Snapshot {
        self.begin();
        let mut b = self.pool.acquire_beyond_budget();
        let root = SnapshotRoot::Value(unsafe { self.add(shapes, &mut b.leaves(), value, 0) });
        unsafe { self.finish(shapes, b, root) }
    }
    pub fn text(&mut self, value: &str) -> Snapshot {
        self.begin();
        let mut b = self.pool.acquire_beyond_budget();
        let root = b.leaves().string_value(&value.into());
        b.finish(SnapshotRoot::Value(root), &mut self.shaper)
    }
    pub fn type_args(&mut self, args: &[(&str, &Type)]) -> Snapshot {
        self.begin();
        let mut b = self.pool.acquire_beyond_budget();
        let fields: Vec<_> = args
            .iter()
            .map(|(name, ty)| {
                let ty = ty.owned(&mut b.leaves(), &mut self.carried);
                (*name, SnapshotValue::Type(b.leaves().ty(ty)))
            })
            .collect();
        let key = b.leaves().ty(OwnedType::String);
        let value = b.leaves().ty(OwnedType::Type);
        let object = b.map(key, value, fields.iter(), |leaves, (key, value)| {
            (leaves.string_value(&(*key).into()), *value)
        });
        let root = b.leaves().object(object).map_or(
            SnapshotValue::Truncated(Limit::Objects),
            SnapshotValue::Object,
        );
        b.finish(SnapshotRoot::Value(root), &mut self.shaper)
    }
    fn begin(&mut self) {
        self.seen.clear();
        self.work.clear();
        self.carried.begin();
    }
    unsafe fn add(
        &mut self,
        shapes: &[Shape],
        leaves: &mut Leaves<'_>,
        value: Dyn,
        depth: usize,
    ) -> SnapshotValue {
        if value.tag == u64::from(u32::MAX - 1) {
            return if leaves.limits().max_depth.is_some_and(|limit| depth > limit) {
                SnapshotValue::Truncated(Limit::Depth)
            } else {
                SnapshotValue::Float(f64::from_bits(value.payload))
            };
        }
        let key = if value.tag >= 3
            && value.tag != u64::MAX
            && matches!(
                shapes[usize::try_from(value.tag - 3).expect("descriptor index")],
                Shape::Enum { .. }
            ) {
            Key::Enum(value.tag, value.payload)
        } else {
            Key::Value(usize::try_from(value.payload).expect("pointer or variant index"))
        };
        if value.tag >= 3
            && let Some(value) = self.seen.get(&key)
        {
            return *value;
        }
        if leaves.limits().max_depth.is_some_and(|limit| depth > limit) {
            return SnapshotValue::Truncated(Limit::Depth);
        }
        match value.tag {
            0 => return SnapshotValue::Int(value.payload.cast_signed()),
            1 => return SnapshotValue::Bool(value.payload != 0),
            2 => return SnapshotValue::Null,
            u64::MAX => return SnapshotValue::OmittedArg,
            _ => {}
        }
        let shape = &shapes[usize::try_from(value.tag - 3).expect("descriptor index")];
        if let Shape::Enum { variants, .. } = shape {
            let result = match self.declaration(shapes, leaves, value.tag, depth + 1) {
                SnapshotValue::Object(declaration) => {
                    match leaves.label(
                        &variants
                            [usize::try_from(value.payload).expect("pointer or variant index")]
                        .into(),
                    ) {
                        Some(name) => SnapshotValue::Enum {
                            declaration,
                            variant: u32::try_from(value.payload).expect("variant index"),
                            name,
                        },
                        None => SnapshotValue::Truncated(Limit::Bytes),
                    }
                }
                value => value,
            };
            self.seen.insert(key, result);
            return result;
        }
        if value.payload == 0 {
            return SnapshotValue::Null;
        }
        let result = if matches!(shape, Shape::Bigint) {
            leaves.bigint(&std::sync::Arc::new(unsafe {
                crate::tasks::bigint_value(value.payload as usize)
            }))
        } else if matches!(shape, Shape::String) {
            let text = unsafe { crate::strings::Str::from_raw(value.payload as usize) };
            // Match the shared capture builder's leaf gate before copying a
            // potentially enormous lazy rope into host memory.
            if text.byte_len() > leaves.limits().max_leaf_bytes {
                SnapshotValue::Truncated(Limit::Bytes)
            } else {
                leaves.string_value(&text.to_owned().into())
            }
        } else {
            let Some(slot) = leaves.reserve() else {
                return SnapshotValue::Truncated(Limit::Objects);
            };
            let value_id = SnapshotValue::Object(slot.id());
            self.work.push(Work {
                slot,
                key,
                tag: value.tag,
                depth,
            });
            value_id
        };
        self.seen.insert(key, result);
        result
    }
    fn declaration(
        &mut self,
        shapes: &[Shape],
        leaves: &mut Leaves<'_>,
        tag: u64,
        depth: usize,
    ) -> SnapshotValue {
        let declaration = match &shapes[usize::try_from(tag - 3).expect("descriptor index")] {
            Shape::Class { declaration, .. }
            | Shape::GenericClass { declaration, .. }
            | Shape::Enum { declaration, .. } => declaration(),
            _ => unreachable!("declaration shape"),
        };
        let key = Key::Declaration(declaration.tag.as_i64().cast_unsigned());
        if let Some(value) = self.seen.get(&key) {
            return *value;
        }
        if leaves.limits().max_depth.is_some_and(|limit| depth > limit) {
            return SnapshotValue::Truncated(Limit::Depth);
        }
        let Some(slot) = leaves.reserve() else {
            return SnapshotValue::Truncated(Limit::Objects);
        };
        let value = SnapshotValue::Object(slot.id());
        self.work.push(Work {
            slot,
            key,
            tag,
            depth,
        });
        self.seen.insert(key, value);
        value
    }
    unsafe fn finish(&mut self, shapes: &[Shape], mut b: Builder, root: SnapshotRoot) -> Snapshot {
        while let Some(Work {
            slot,
            key,
            tag,
            depth,
        }) = self.work.pop()
        {
            let shape = &shapes[usize::try_from(tag - 3).expect("descriptor index")];
            let object = match (key, shape) {
                (
                    Key::Declaration(_),
                    Shape::Class { declaration, .. }
                    | Shape::GenericClass { declaration, .. }
                    | Shape::Enum { declaration, .. },
                ) => {
                    let declaration = declaration();
                    let definition = b.define(&declaration.definition, &mut self.carried);
                    b.declaration(
                        &declaration.name,
                        declaration.tag,
                        declaration.is_enum,
                        Some(definition),
                    )
                }
                (
                    Key::Value(pointer),
                    Shape::Class { fields, .. } | Shape::GenericClass { fields, .. },
                ) => match self.declaration(shapes, &mut b.leaves(), tag, depth + 1) {
                    SnapshotValue::Object(declaration) => {
                        let args = if let Shape::GenericClass { args, .. } = shape {
                            args.iter()
                                .map(|t| t.owned(&mut b.leaves(), &mut self.carried))
                                .collect::<Vec<_>>()
                        } else {
                            Vec::new()
                        };
                        b.instance(declaration, args, fields.iter(), |leaves, field| {
                            let ptr = unsafe { (pointer as *const u8).add(field.offset) };
                            let value = match field.slot {
                                SlotKind::Tagged => unsafe { ptr.cast::<Dyn>().read() },
                                _ => Dyn {
                                    tag: u64::from(field.type_id),
                                    payload: unsafe { ptr.cast::<u64>().read() },
                                },
                            };
                            let captured = if field.type_id == u32::MAX - 2 {
                                let key = Key::Value(ptr as usize);
                                if let Some(value) = self.seen.get(&key) {
                                    *value
                                } else {
                                    let value = leaves
                                        .object(SnapshotObject::NonSnapshotableValue {})
                                        .map_or(
                                            SnapshotValue::Truncated(Limit::Objects),
                                            SnapshotValue::Object,
                                        );
                                    self.seen.insert(key, value);
                                    value
                                }
                            } else {
                                unsafe { self.add(shapes, leaves, value, depth + 1) }
                            };
                            (field.name.into(), captured)
                        })
                    }
                    SnapshotValue::Truncated(reason) => SnapshotObject::Truncated(reason),
                    _ => unreachable!("declaration identity"),
                },
                (
                    Key::Value(pointer),
                    Shape::List {
                        growing,
                        tag,
                        element,
                    },
                ) => {
                    let ptr = pointer as *const usize;
                    let (data, len) = if *growing {
                        let buffer = unsafe { ptr.add(1).read() } as *const usize;
                        (buffer.wrapping_add(2), unsafe { ptr.add(2).read() })
                    } else {
                        (unsafe { ptr.add(2) }, unsafe { ptr.add(1).read() })
                    };
                    let ty = element_type(
                        element,
                        unsafe { (*(pointer as *const baml_rt_gc::Header)).aux },
                        &mut b.leaves(),
                        &mut self.carried,
                    );
                    let ty = b.leaves().ty(ty);
                    b.list(ty, 0..len, |leaves, i| unsafe {
                        self.add(
                            shapes,
                            leaves,
                            Dyn {
                                tag: *tag,
                                payload: data.add(i).read() as u64,
                            },
                            depth + 1,
                        )
                    })
                }
                (Key::Value(pointer), Shape::Uint8Array) => {
                    b.bytes(unsafe { crate::bytes::data(pointer) })
                }
                (Key::Value(_), Shape::RustType) => SnapshotObject::NonSnapshotableValue {},
                (Key::Value(pointer), Shape::MapStringPtr { tag, value }) => {
                    let ptr = pointer as *const usize;
                    let data = unsafe { ptr.add(1).read() as *const usize }.wrapping_add(2);
                    let len = unsafe { ptr.add(2).read() };
                    let key_type = b.leaves().ty(OwnedType::String);
                    let value = element_type(
                        value,
                        unsafe { (*(pointer as *const baml_rt_gc::Header)).aux },
                        &mut b.leaves(),
                        &mut self.carried,
                    );
                    let value_type = b.leaves().ty(value);
                    b.map(key_type, value_type, 0..len, |leaves, i| unsafe {
                        (
                            self.add(
                                shapes,
                                leaves,
                                Dyn {
                                    tag: 3,
                                    payload: data.add(i * 2).read() as u64,
                                },
                                depth + 1,
                            ),
                            self.add(
                                shapes,
                                leaves,
                                Dyn {
                                    tag: *tag,
                                    payload: data.add(i * 2 + 1).read() as u64,
                                },
                                depth + 1,
                            ),
                        )
                    })
                }
                (Key::Value(pointer), Shape::MapStringInt) => {
                    let ptr = pointer as *const usize;
                    let data = unsafe { ptr.add(1).read() as *const usize }.wrapping_add(2);
                    let len = unsafe { ptr.add(2).read() };
                    let key_type = b.leaves().ty(OwnedType::String);
                    let value_type = b.leaves().ty(OwnedType::Int);
                    b.map(key_type, value_type, 0..len, |leaves, i| unsafe {
                        (
                            self.add(
                                shapes,
                                leaves,
                                Dyn {
                                    tag: 3,
                                    payload: data.add(i * 2).read() as u64,
                                },
                                depth + 1,
                            ),
                            SnapshotValue::Int((data.add(i * 2 + 1).read() as u64).cast_signed()),
                        )
                    })
                }
                (Key::Value(_), Shape::Future) => SnapshotObject::Descriptive {
                    kind: btel_snapshot::Description::Future,
                    name: None,
                },
                (Key::Value(pointer), Shape::Closure) => {
                    let name = unsafe { crate::closures::name(pointer) };
                    SnapshotObject::Descriptive {
                        kind: if unsafe { crate::closures::is_generic(pointer) } {
                            btel_snapshot::Description::GenericFunction
                        } else if name.is_some() {
                            btel_snapshot::Description::Function
                        } else if unsafe { crate::closures::is_bound(pointer) } {
                            btel_snapshot::Description::BoundMethod
                        } else {
                            btel_snapshot::Description::Closure
                        },
                        name: name.and_then(|name| b.leaves().label(&name.into())),
                    }
                }
                _ => unreachable!("compiler admitted an unsupported capture shape"),
            };
            b.fill(slot, object);
        }
        let snapshot = b.finish(root, &mut self.shaper);
        self.seen.clear();
        snapshot
    }
}

impl Declaration {
    pub fn from_bytes(name: &[u8], tag: i64, definition: Definition, is_enum: bool) -> Self {
        use borsh::BorshDeserialize;
        Self {
            name: baml_type::DeclarationName::try_from_slice(name).expect("compiler declaration"),
            tag: baml_type::typetag::TypeTag::from_i64(tag),
            definition,
            is_enum,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, OnceLock};

    fn declaration() -> &'static Declaration {
        static DECL: OnceLock<Declaration> = OnceLock::new();
        DECL.get_or_init(|| Declaration {
            name: baml_type::DeclarationName::Declared(baml_type::TypeName::from_dotted_path(
                "test.Node",
            )),
            tag: baml_type::typetag::TypeTag::from_i64(123),
            definition: Definition {
                group: Arc::new(btel_types::DefinitionBlob::new(
                    [42; 16],
                    Box::new([]),
                    Box::new([]),
                )),
                member: 0,
            },
            is_enum: false,
        })
    }
    const FIELDS: &[Field] = &[Field {
        name: "next",
        offset: 8,
        slot: SlotKind::Tagged,
        type_id: 0,
    }];
    static SHAPES: &[Shape] = &[
        Shape::String,
        Shape::Class {
            declaration,
            fields: FIELDS,
        },
        Shape::Class {
            declaration,
            fields: FIELDS,
        },
        Shape::List {
            growing: false,
            tag: 0,
            element: &Type::Int,
        },
        Shape::List {
            growing: true,
            tag: 3,
            element: &Type::String,
        },
        Shape::MapStringInt,
        Shape::Enum {
            declaration,
            variants: &["Declared", "Aliased"],
        },
    ];
    fn builder() -> Builder {
        SnapshotPool::unbounded(Limits::default()).acquire_beyond_budget()
    }

    #[test]
    fn primitive_arguments_and_omission_have_canonical_identity() {
        let values = [
            Dyn {
                tag: 0,
                payload: (-19i64).cast_unsigned(),
            },
            Dyn { tag: 1, payload: 1 },
            Dyn { tag: 2, payload: 0 },
            Dyn {
                tag: u64::MAX,
                payload: 0,
            },
        ];
        let actual = unsafe { Scratch::default().arguments(SHAPES, &values) };
        let mut b = builder();
        let root = b.arguments(
            [
                SnapshotValue::Int(-19),
                SnapshotValue::Bool(true),
                SnapshotValue::Null,
                SnapshotValue::OmittedArg,
            ]
            .into_iter(),
            |_, v| v,
        );
        let expected = b.finish(root, &mut Shaper::default());
        assert_eq!(actual.root_id(), expected.root_id());
    }
    #[test]
    fn cycles_aliases_and_duplicate_layouts_share_declaration_identity() {
        #[repr(C)]
        struct Node {
            header: u64,
            next: Dyn,
        }
        let mut a = Box::new(Node {
            header: 0,
            next: Dyn { tag: 2, payload: 0 },
        });
        let mut b = Box::new(Node {
            header: 0,
            next: Dyn { tag: 2, payload: 0 },
        });
        let av = Dyn {
            tag: 4,
            payload: (&raw mut *a) as u64,
        };
        let bv = Dyn {
            tag: 5,
            payload: (&raw mut *b) as u64,
        };
        a.next = bv;
        b.next = av;
        let actual = unsafe { Scratch::default().arguments(SHAPES, &[av, av, bv]) };
        let mut b = builder();
        let first = b.leaves().reserve().unwrap();
        let second = b.leaves().reserve().unwrap();
        let decl = b.leaves().reserve().unwrap();
        let a_id = first.id();
        let b_id = second.id();
        let d_id = decl.id();
        let root = b.arguments([a_id, a_id, b_id].into_iter(), |_, id| {
            SnapshotValue::Object(id)
        });
        let object = b.instance(
            d_id,
            [],
            [("next", SnapshotValue::Object(b_id))].into_iter(),
            |_, (k, v)| (k.into(), v),
        );
        b.fill(first, object);
        let object = b.instance(
            d_id,
            [],
            [("next", SnapshotValue::Object(a_id))].into_iter(),
            |_, (k, v)| (k.into(), v),
        );
        b.fill(second, object);
        let d = declaration();
        let definition = b.define(&d.definition, &mut Carried::default());
        let object = b.declaration(&d.name, d.tag, false, Some(definition));
        b.fill(decl, object);
        let expected = b.finish(root, &mut Shaper::default());
        assert_eq!(actual.root_id(), expected.root_id());
    }
    #[test]
    fn empty_buffers_and_nullable_pointers_are_safe_and_typed() {
        let fixed = [0usize, 0];
        let growing = [0usize, 0, 0, 0];
        for (tag, ptr, element) in [
            (6, fixed.as_ptr(), OwnedType::Int),
            (7, growing.as_ptr(), OwnedType::String),
        ] {
            let actual = unsafe {
                Scratch::default().value(
                    SHAPES,
                    Dyn {
                        tag,
                        payload: ptr as u64,
                    },
                )
            };
            let mut b = builder();
            let ty = b.leaves().ty(element);
            let object = b.list(ty, [].into_iter(), |_, v: SnapshotValue| v);
            let id = b.leaves().object(object).unwrap();
            let expected = b.finish(
                SnapshotRoot::Value(SnapshotValue::Object(id)),
                &mut Shaper::default(),
            );
            assert_eq!(actual.root_id(), expected.root_id());
        }
        let actual = unsafe {
            Scratch::default().value(
                SHAPES,
                Dyn {
                    tag: 8,
                    payload: growing.as_ptr() as u64,
                },
            )
        };
        let mut b = builder();
        let key = b.leaves().ty(OwnedType::String);
        let value = b.leaves().ty(OwnedType::Int);
        let object = b.map(
            key,
            value,
            [].into_iter(),
            |_, v: (SnapshotValue, SnapshotValue)| v,
        );
        let id = b.leaves().object(object).unwrap();
        let expected = b.finish(
            SnapshotRoot::Value(SnapshotValue::Object(id)),
            &mut Shaper::default(),
        );
        assert_eq!(actual.root_id(), expected.root_id());
        for tag in 3..9 {
            assert!(matches!(
                unsafe { Scratch::default().value(SHAPES, Dyn { tag, payload: 0 }) }.value(),
                Some(SnapshotValue::Null)
            ));
        }
        assert!(matches!(
            unsafe { Scratch::default().value(SHAPES, Dyn { tag: 9, payload: 0 }) }.value(),
            Some(SnapshotValue::Enum { variant: 0, .. })
        ));
    }
}

fn named_identity(tag: i64, name: &[u8]) -> TypeIdentity {
    TypeIdentity::Resolved(baml_type::TaggedTypeName::new(
        baml_type::typetag::TypeTag::from_i64(tag),
        borsh::from_slice(name).expect("frozen type name"),
    ))
}
