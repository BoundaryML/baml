//! Pack-specific immutable schemas for the owned native-provider boundary.
use super::*;
use baml_builtins2_codegen::BamlType as Native;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Scalar(Repr),
    Class(usize),
    Enum(Repr, usize),
    Optional(Box<Key>),
    Union(Vec<Key>),
    List(Repr, Box<Key>),
    Map(Repr, Box<Key>, Box<Key>),
}
impl Key {
    fn repr(&self) -> Repr {
        match self {
            Self::Scalar(r) | Self::Enum(r, _) | Self::List(r, _) | Self::Map(r, ..) => *r,
            Self::Class(c) => Repr::Ptr(*c),
            Self::Optional(inner) if inner.repr().pointer() => inner.repr(),
            Self::Optional(_) | Self::Union(_) => Repr::Tagged,
        }
    }
}
pub(super) struct Schemas<'a, 'p, 'db> {
    e: &'a Emitter<'p, 'db>,
    keys: Vec<Key>,
    definitions: Vec<String>,
}
impl<'a, 'p, 'db> Schemas<'a, 'p, 'db> {
    pub(super) fn new(e: &'a Emitter<'p, 'db>) -> Self {
        Self {
            e,
            keys: Vec::new(),
            definitions: Vec::new(),
        }
    }
    fn name(&self, c: usize) -> Gen<String> {
        let origin = self.e.plan.class_origin(c);
        let p = self
            .e
            .plan
            .program
            .packages
            .iter()
            .find(|p| p.objects.contains(&(origin as u32)))
            .ok_or_else(|| no("native class package"))?;
        Ok(
            match &self.e.plan.program.objects[bex_vm_types::ObjectIndex::from_raw(origin)] {
                Object::Class(c) => format!("{}.{}", p.name, c.path),
                Object::Enum(c) => format!("{}.{}", p.name, c.path),
                _ => return Err(no("native boundary declaration")),
            },
        )
    }
    fn from_repr(&self, r: Repr) -> Gen<Key> {
        Ok(match r {
            Repr::Ptr(c) => Key::Class(c),
            Repr::Enum(c) => Key::Enum(r, c),
            r @ (Repr::Null
            | Repr::Int
            | Repr::Float
            | Repr::Bool
            | Repr::Str
            | Repr::Bigint
            | Repr::Bytes
            | Repr::Opaque) => Key::Scalar(r),
            r @ (Repr::ListInt | Repr::ListStr | Repr::ListPtr(_) | Repr::ListListStr) => {
                Key::List(r, Box::new(self.from_repr(self.e.plan.element(r)?)?))
            }
            _ => return Err(no(format!("native boundary storage {r:?}"))),
        })
    }
    fn field(&self, ty: &bex_vm_types::RuntimeTy) -> Gen<Key> {
        use bex_vm_types::RuntimeTy as T;
        Ok(match ty {
            T::Union(m) => {
                let mut keys = m.iter().map(|t| self.field(t)).collect::<Gen<Vec<_>>>()?;
                keys.sort();
                keys.dedup();
                if keys.len() == 1 {
                    keys.pop().unwrap()
                } else if keys.len() == 2 && keys.contains(&Key::Scalar(Repr::Null)) {
                    Key::Optional(Box::new(
                        keys.into_iter()
                            .find(|k| *k != Key::Scalar(Repr::Null))
                            .unwrap(),
                    ))
                } else {
                    Key::Union(keys)
                }
            }
            T::List(element) => {
                Key::List(self.e.plan.field_repr(ty)?, Box::new(self.field(element)?))
            }
            T::Map { key, value } => Key::Map(
                self.e.plan.field_repr(ty)?,
                Box::new(self.field(key)?),
                Box::new(self.field(value)?),
            ),
            _ => self.from_repr(self.e.plan.field_repr(ty)?)?,
        })
    }
    fn native(&self, ty: &Native, hint: Option<Repr>) -> Gen<Key> {
        Ok(match ty {
            Native::Null => Key::Scalar(Repr::Null),
            Native::Int => Key::Scalar(Repr::Int),
            Native::Float => Key::Scalar(Repr::Float),
            Native::Bool => Key::Scalar(Repr::Bool),
            Native::String => Key::Scalar(Repr::Str),
            Native::Bigint => Key::Scalar(Repr::Bigint),
            Native::Uint8Array => Key::Scalar(Repr::Bytes),
            Native::RustType => Key::Scalar(Repr::Opaque),
            Native::Optional(inner) => Key::Optional(Box::new(
                self.native(inner, hint.filter(|r| *r != Repr::Null))?,
            )),
            Native::List(element) => {
                let element = self.native(element, None)?;
                let r = hint
                    .filter(|r| self.e.lists.contains_key(r))
                    .or_else(|| {
                        self.e
                            .plan
                            .lists
                            .iter()
                            .copied()
                            .find(|r| self.e.plan.element(*r).ok() == Some(element.repr()))
                    })
                    .ok_or_else(|| no("native list layout"))?;
                Key::List(r, Box::new(element))
            }
            Native::Map(key, value) => {
                let k = self.native(key, None)?;
                let v = self.native(value, None)?;
                let r =
                    hint.filter(|r| matches!(r, Repr::MapStrInt | Repr::Map(_)))
                        .or_else(|| {
                            self.e.plan.lists.iter().copied().find(|r| {
                                self.e.plan.map_parts(*r).ok() == Some((k.repr(), v.repr()))
                            })
                        })
                        .ok_or_else(|| no("native map layout"))?;
                Key::Map(r, Box::new(k), Box::new(v))
            }
            Native::Named(name) => {
                if matches!(name.as_str(), "unknown" | "union") {
                    return self.from_repr(hint.ok_or_else(|| no("native dynamic type"))?);
                }
                if let Some(c) = self.e.plan.layouts.keys().copied().find(|c| {
                    self.e.plan.class(*c).path.to_string().rsplit('.').next() == Some(name.as_str())
                }) {
                    Key::Class(c)
                } else {
                    self.from_repr(hint.ok_or_else(|| no(format!("native class {name}")))?)?
                }
            }
            _ => return Err(no(format!("native boundary type {ty:?}"))),
        })
    }
    pub(super) fn declared(&mut self, ty: &Native, hint: Option<Repr>) -> Gen<String> {
        let key = self.native(ty, hint)?;
        self.register(key)
    }
    pub(super) fn object(&mut self, c: usize) -> Gen<String> {
        self.register(Key::Class(c))
    }
    fn wire(&self, key: &Key) -> Gen<String> {
        Ok(match key {
            Key::Scalar(r) => format!(
                "rt::external::WireTy::{}",
                match r {
                    Repr::Null => "Null",
                    Repr::Int => "Int",
                    Repr::Float => "Float",
                    Repr::Bool => "Bool",
                    Repr::Str => "String",
                    Repr::Bigint => "Bigint",
                    Repr::Bytes => "Uint8Array",
                    _ => "Unknown",
                }
            ),
            Key::Class(c) => format!(
                "rt::external::WireTy::Class(rt::provider::baml_type::WireHead::Static(rt::provider::baml_type::TypeName::from_dotted_path({:?})),Box::new([]))",
                self.name(*c)?
            ),
            Key::Enum(_, c) => format!(
                "rt::external::WireTy::Enum(rt::provider::baml_type::WireHead::Static(rt::provider::baml_type::TypeName::from_dotted_path({:?})))",
                self.name(*c)?
            ),
            Key::Optional(t) => format!(
                "rt::external::WireTy::Union(vec![rt::external::WireTy::Null,{}].into_boxed_slice())",
                self.wire(t)?
            ),
            Key::Union(types) => format!(
                "rt::external::WireTy::Union(vec![{}].into_boxed_slice())",
                types
                    .iter()
                    .map(|t| self.wire(t))
                    .collect::<Gen<Vec<_>>>()?
                    .join(",")
            ),
            Key::List(_, t) => format!("rt::external::WireTy::List(Box::new({}))", self.wire(t)?),
            Key::Map(_, k, v) => format!(
                "rt::external::WireTy::Map{{key:Box::new({}),value:Box::new({})}}",
                self.wire(k)?,
                self.wire(v)?
            ),
        })
    }
    fn register(&mut self, key: Key) -> Gen<String> {
        if let Some(i) = self.keys.iter().position(|k| k == &key) {
            return Ok(format!("IO_T{i}"));
        }
        let i = self.keys.len();
        self.keys.push(key.clone());
        let tag = match key.repr() {
            Repr::Enum(c) => format!("CAPTURE_ENUM_{c}_TAG"),
            Repr::Tagged => "u64::MAX".into(),
            r => self.e.tag(r)?.to_string(),
        };
        let wire = self.wire(&key)?;
        let shape = match &key {
            Key::Scalar(r) => format!(
                "rt::external::Shape::{}",
                match r {
                    Repr::Null => "Null",
                    Repr::Int => "Int",
                    Repr::Float => "Float",
                    Repr::Bool => "Bool",
                    Repr::Str => "String",
                    Repr::Bigint => "Bigint",
                    Repr::Bytes => "Bytes",
                    Repr::Opaque => "Opaque",
                    _ => unreachable!(),
                }
            ),
            Key::Optional(t) => format!(
                "rt::external::Shape::Optional(&{})",
                self.register((**t).clone())?
            ),
            Key::Union(types) => {
                let members = types
                    .iter()
                    .map(|t| self.register(t.clone()).map(|t| format!("&{t}")))
                    .collect::<Gen<Vec<_>>>()?;
                format!("rt::external::Shape::Union(&[{}])", members.join(","))
            }
            Key::Enum(_, c) => {
                let Object::Enum(e) =
                    &self.e.plan.program.objects[bex_vm_types::ObjectIndex::from_raw(*c)]
                else {
                    return Err(no("native enum schema"));
                };
                format!(
                    "rt::external::Shape::Enum{{name:{:?},variants:&{:?}}}",
                    self.name(*c)?,
                    e.variants.iter().map(|v| &v.name).collect::<Vec<_>>()
                )
            }
            Key::Class(c) => {
                let layout = &self.e.plan.layouts[c];
                let class = self.e.plan.class(*c);
                let mut fields = Vec::new();
                for (j, field) in class.fields.iter().enumerate() {
                    let child = self.register(self.field(&field.field_type)?)?;
                    let r = layout.fields[j];
                    let slot = if r.pointer() {
                        "Pointer"
                    } else if matches!(r, Repr::Tagged | Repr::Dyn(_)) {
                        "Tagged"
                    } else {
                        "Scalar"
                    };
                    fields.push(format!("rt::external::Field{{name:{:?},offset:{},slot:baml_rt_gc::SlotKind::{slot},ty:&{child}}}",field.name,layout.offsets[j]));
                }
                self.definitions.push(format!("static IO_C{i}:rt::external::Class=rt::external::Class{{name:{:?},desc:{},bytes:{},fields:&[{}]}};",self.name(*c)?,self.e.descriptors[c],layout.bytes,fields.join(",")));
                format!("rt::external::Shape::Class(&IO_C{i})")
            }
            Key::List(r, t) => {
                let child = self.register((**t).clone())?;
                let element = t.repr();
                let slot = if element.pointer() {
                    "Pointer"
                } else if matches!(element, Repr::Tagged | Repr::Dyn(_)) {
                    "Tagged"
                } else {
                    "Scalar"
                };
                let stride = if slot == "Tagged" { 16 } else { 8 };
                format!(
                    "rt::external::Shape::List{{desc:{},growing:{},element:&{child},slot:baml_rt_gc::SlotKind::{slot},stride:{stride}}}",
                    self.e.lists[r],
                    r.growing()
                )
            }
            Key::Map(r, k, v) => {
                if k.repr() != Repr::Str {
                    return Err(no("native IO map requires string keys"));
                }
                let child = self.register((**v).clone())?;
                let slot = maps::slot_type(v.repr(), self.e.descriptors, self.e.lists)?;
                self.definitions.push(format!("unsafe fn IO_MAP_OUT{i}(p:usize,n:usize)->R<rt::external::Owned>{{unsafe{{rt::external::copy_string_map::<{slot}>(p,&{child},n)}}}} unsafe fn IO_MAP_IN{i}(v:&rt::external::Owned,n:usize)->R<usize>{{unsafe{{rt::external::hydrate_string_map::<{slot}>(v,&{child},{},n)}}}}",self.e.lists[r]));
                format!("rt::external::Shape::Map{{encode:IO_MAP_OUT{i},hydrate:IO_MAP_IN{i}}}")
            }
        };
        self.definitions.push(format!("static IO_T{i}:rt::external::Type=rt::external::Type{{tag:{tag},wire:||{wire},shape:{shape}}};"));
        Ok(format!("IO_T{i}"))
    }
    pub(super) fn finish(self) -> String {
        self.definitions.join("\n")
    }
}
