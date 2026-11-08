use super::*;

impl Plan<'_> {
    pub(super) fn class_tag(&self, tag: baml_type::typetag::TypeTag) -> Gen<usize> {
        self.program
            .objects
            .iter()
            .enumerate()
            .find_map(|(i, o)| match o {
                Object::Class(c) if c.type_tag == tag => Some(i),
                _ => None,
            })
            .ok_or_else(|| no("unlinked class tag"))
    }
    pub(super) fn record_ty(&mut self, ty: &RuntimeTy) -> Gen<()> {
        if let RuntimeTy::Union(members) = ty {
            for member in members {
                if !recursive_container(member) {
                    self.record_ty(member)?;
                }
            }
        } else if !matches!(ty, RuntimeTy::Function { .. }) {
            self.record(self.repr(ty)?)?;
        }
        Ok(())
    }
    pub(super) fn record_field_ty(&mut self, ty: &bex_vm_types::RuntimeTy) -> Gen<()> {
        if let bex_vm_types::RuntimeTy::Union(members) = ty {
            for member in members {
                if !recursive_container(member) {
                    self.record_field_ty(member)?;
                }
            }
        } else {
            self.record(self.field_repr(ty)?)?;
        }
        Ok(())
    }
    pub(super) fn record(&mut self, repr: Repr) -> Gen<()> {
        match repr {
            Repr::Enum(e) => {
                self.enums.insert(e);
            }
            Repr::Ptr(c) => self.add(c)?,
            Repr::ListPtr(c) => {
                self.lists.insert(repr);
                self.add(c)?;
            }
            Repr::MapStrPtr(c) => {
                self.lists.insert(repr);
                self.record(Repr::ListPtr(c))?;
                self.record(Repr::ListStr)?;
            }
            Repr::MapStrInt => {
                self.lists.insert(repr);
                self.record(Repr::ListInt)?;
            }
            Repr::ListInt | Repr::ListStr => {
                self.lists.insert(repr);
            }
            Repr::ListListStr => {
                self.lists.insert(repr);
                self.record(Repr::ListStr)?;
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn repr(&self, ty: &RuntimeTy) -> Gen<Repr> {
        Ok(match ty {
            RuntimeTy::Float | RuntimeTy::Literal(baml_type::Literal::Float(_), _) => Repr::Float,
            RuntimeTy::Int | RuntimeTy::Literal(baml_type::Literal::Int(_), _) => Repr::Int,
            RuntimeTy::Bool | RuntimeTy::Literal(baml_type::Literal::Bool(_), _) => Repr::Bool,
            RuntimeTy::String | RuntimeTy::Literal(baml_type::Literal::String(_), _) => Repr::Str,
            RuntimeTy::Unknown | RuntimeTy::TypeAlias(_) => Repr::Tagged,
            RuntimeTy::Uint8Array => Repr::Bytes,
            RuntimeTy::RustType => Repr::Opaque,
            RuntimeTy::Map { key, value }
                if matches!(key.as_ref(), RuntimeTy::String)
                    && matches!(value.as_ref(), RuntimeTy::Int) =>
            {
                Repr::MapStrInt
            }
            RuntimeTy::Map { key, value } if matches!(key.as_ref(), RuntimeTy::String) => {
                match self.repr(value)? {
                    Repr::Ptr(c) => Repr::MapStrPtr(c),
                    _ => return Err(no("map value")),
                }
            }
            RuntimeTy::Null => Repr::Null,
            RuntimeTy::List(element) => match element.as_ref() {
                RuntimeTy::Int | RuntimeTy::Literal(baml_type::Literal::Int(_), _) => Repr::ListInt,
                RuntimeTy::String | RuntimeTy::Literal(baml_type::Literal::String(_), _) => {
                    Repr::ListStr
                }
                RuntimeTy::List(t) if matches!(t.as_ref(), RuntimeTy::String) => Repr::ListListStr,
                RuntimeTy::Union(_) | RuntimeTy::Class(..) => match self.repr(element)? {
                    Repr::Ptr(c) => Repr::ListPtr(c),
                    _ => return Err(no("list element")),
                },
                _ => return Err(no("list element")),
            },
            RuntimeTy::Interface(head, _, _) => Repr::Dyn(
                self.index
                    .interface_at(head)
                    .ok_or_else(|| no("interface"))?,
            ),
            RuntimeTy::Enum(head) | RuntimeTy::EnumVariant(head, _)
                if Some(head.root()) == self.lang.get(baml_base::LangPackage::Baml)
                    && head.namespace().as_slice() == ["json"]
                    && head.name().as_str() == "Naming" =>
            {
                Repr::Naming
            }
            RuntimeTy::Enum(head) | RuntimeTy::EnumVariant(head, _) => Repr::Enum(
                self.index
                    .enum_at(head)
                    .ok_or_else(|| no("unlinked enum"))?,
            ),
            RuntimeTy::Class(head, args)
                if args.is_empty()
                    && head.builtin_primitive(self.lang)
                        == Some(baml_type::PrimitiveType::Uint8Array) =>
            {
                Repr::Bytes
            }
            RuntimeTy::Class(head, args)
                if args.is_empty()
                    && head.builtin_primitive(self.lang)
                        == Some(baml_type::PrimitiveType::String) =>
            {
                Repr::Str
            }
            RuntimeTy::Class(head, args) if !args.is_empty() => {
                let template = baml_compiler2_mir::TyTemplate::from(
                    baml_compiler2_mir::RealizedTy::try_from(ty.clone())
                        .map_err(|_| no("unrealized class"))?,
                );
                let anchored = self
                    .index
                    .anchor(&template)
                    .ok_or_else(|| no("generic class head"))?;
                self.field_repr(&anchored.substitute_symbolic(&[]))?
            }
            RuntimeTy::Class(head, args) if args.is_empty() => Repr::Ptr(
                self.index
                    .class_at(head)
                    .ok_or_else(|| no(format!("unlinked class {head:?}")))?
                    .0,
            ),
            RuntimeTy::Union(members)
                if members.len() == 2 && members.iter().any(|m| matches!(m, RuntimeTy::Null)) =>
            {
                let member = members
                    .iter()
                    .find(|m| !matches!(m, RuntimeTy::Null))
                    .unwrap();
                let repr = self.repr(member)?;
                if repr.pointer() { repr } else { Repr::Tagged }
            }
            RuntimeTy::Union(members) => {
                for member in members {
                    if !recursive_container(member) {
                        self.repr(member)?;
                    }
                }
                Repr::Tagged
            }
            _ => return Err(no(format!("type {ty:?}"))),
        })
    }
    pub(super) fn field_repr(&self, ty: &bex_vm_types::RuntimeTy) -> Gen<Repr> {
        use bex_vm_types::RuntimeTy as T;
        Ok(match ty {
            T::Null => Repr::Null,
            T::Unknown | T::TypeAlias(_) => Repr::Tagged,
            T::Uint8Array => Repr::Bytes,
            T::RustType => Repr::Opaque,
            T::Enum(head) | T::EnumVariant(head, _)
                if self.program.packages.iter().any(|p| {
                    p.enums.iter().any(|(path, index)| {
                        path.under(&p.name).to_string() == "baml.json.Naming"
                            && matches!(&self.program.objects[*index], Object::Enum(e) if e.type_tag == head.tag())
                    })
                }) => Repr::Naming,
            T::Enum(head) | T::EnumVariant(head, _) => Repr::Enum(
                self.program
                    .objects
                    .iter()
                    .enumerate()
                    .find_map(|(i, o)| match o {
                        Object::Enum(e) if e.type_tag == head.tag() => Some(i),
                        _ => None,
                    })
                    .ok_or_else(|| no("enum field"))?,
            ),
            T::Float | T::Literal(baml_type::Literal::Float(_), _) => Repr::Float,
            T::Int | T::Literal(baml_type::Literal::Int(_), _) => Repr::Int,
            T::Bool | T::Literal(baml_type::Literal::Bool(_), _) => Repr::Bool,
            T::String | T::Literal(baml_type::Literal::String(_), _) => Repr::Str,
            T::Map { key, value }
                if matches!(key.as_ref(), T::String) && matches!(value.as_ref(), T::Int) =>
            {
                Repr::MapStrInt
            }
            T::Map {key,value} if matches!(key.as_ref(),T::String)=>match self.field_repr(value)? { Repr::Ptr(c)=>Repr::MapStrPtr(c),_=>return Err(no("map field value")) },
            T::List(element) => match element.as_ref() {
                T::Int | T::Literal(baml_type::Literal::Int(_), _) => Repr::ListInt,
                T::String | T::Literal(baml_type::Literal::String(_), _) => Repr::ListStr,
                T::List(t) if matches!(t.as_ref(), T::String) => Repr::ListListStr,
                T::Union(_) | T::Class(..) => match self.field_repr(element)? {
                    Repr::Ptr(c) => Repr::ListPtr(c),
                    _ => return Err(no("list field element")),
                },
                _ => return Err(no("list field element")),
            },
            T::Interface(head, _, _) => Repr::Dyn(
                self.program
                    .objects
                    .iter()
                    .enumerate()
                    .find_map(|(i, o)| match o {
                        Object::Interface(d) if d.type_tag == head.tag() => Some(i),
                        _ => None,
                    })
                    .ok_or_else(|| no("interface field"))?,
            ),
            T::Class(head, args) if !args.is_empty() => Repr::Ptr(
                self.class_instances
                    .iter()
                    .find(|(c, a, _)| {
                        self.class(*c).type_tag == head.tag() && a.as_slice() == args.as_ref()
                    })
                    .map(|(_, _, c)| *c)
                    .ok_or_else(|| no("unregistered generic class"))?,
            ),
            T::Class(head, args) if args.is_empty() => {
                let object = self
                    .program
                    .objects
                    .iter()
                    .enumerate()
                    .find_map(|(i, o)| match o {
                        Object::Class(c) if c.type_tag == head.tag() => Some(i),
                        _ => None,
                    })
                    .ok_or_else(|| no("unlinked field class"))?;
                Repr::Ptr(object)
            }
            T::Union(m) if m.len() == 2 && m.iter().any(|t| matches!(t, T::Null)) => {
                let repr = self.field_repr(m.iter().find(|t| !matches!(t, T::Null)).unwrap())?;
                if repr.pointer() { repr } else { Repr::Tagged }
            }
            T::Union(members) => {
                for member in members {
                    if !recursive_container(member) {
                        self.field_repr(member)?;
                    }
                }
                Repr::Tagged
            }
            _ => return Err(no(format!("field type {ty:?}"))),
        })
    }
    pub(super) fn add(&mut self, object: usize) -> Gen<()> {
        if self.layouts.contains_key(&object) {
            return Ok(());
        }
        let class = self.class(object);
        // Runtime-produced exception descriptors are reserved. The runtime supports
        // wildcard panic exclusion and user-defined typed catches; never emit a
        // second, incompatible layout/tag for a builtin exception class.
        if self.program.packages.iter().any(|package| {
            package.name == "baml"
                && package
                    .objects
                    .contains(&u32::try_from(object).expect("object index"))
        }) && ((class.path.to_string().starts_with("panics.")
            && class.path.to_string() != "panics.UserPanic")
            || matches!(
                class.path.to_string().as_str(),
                "json.ParseError" | "json.DecodeError"
            ))
        {
            return Err(no("typed builtin exception class"));
        }
        registry::admit(registry::Admission::Class {
            cleanup: class.cleanup.is_some(),
            generic: false,
        })?;
        let field_types = self.class_field_types(object);
        let fields = field_types
            .iter()
            .map(|t| {
                self.field_repr(t).map_err(|e| {
                    format!(
                        "{e} (layout {}, object {object}, types {field_types:?})",
                        class.path
                    )
                })
            })
            .collect::<Gen<Vec<_>>>()?;
        let mut offsets = vec![0; fields.len()];
        let mut offset = 8;
        for group in 0..3 {
            for (i, repr) in fields.iter().enumerate() {
                let g = if repr.pointer() {
                    0
                } else if matches!(repr, Repr::Dyn(_) | Repr::Tagged) {
                    1
                } else {
                    2
                };
                if g == group {
                    offsets[i] = offset;
                    offset += if g == 1 { 16 } else { 8 };
                }
            }
        }
        let children = fields.clone();

        self.layouts.insert(
            object,
            Layout {
                pointers: fields.iter().filter(|r| r.pointer()).count(),
                tagged: fields
                    .iter()
                    .filter(|r| matches!(r, Repr::Dyn(_) | Repr::Tagged))
                    .count(),
                fields,
                offsets,
                bytes: offset,
            },
        );
        for ty in &field_types {
            self.record_field_ty(ty)?;
        }
        for child in children {
            self.record(child)?;
        }
        Ok(())
    }
}

pub(super) fn emit(mut out: &mut String, plan: &Plan<'_>) -> Gen<LayoutIds> {
    let descriptors: BTreeMap<_, _> = plan
        .layouts
        .keys()
        .enumerate()
        .map(|(i, c)| (*c, i + rt_descriptors()))
        .collect();
    let lists: BTreeMap<_, _> = plan
        .lists
        .iter()
        .scan(descriptors.len() + rt_descriptors(), |next, r| {
            let id = *next;
            *next += if matches!(*r, Repr::MapStrInt | Repr::MapStrPtr(_)) {
                3
            } else if r.growing() {
                2
            } else {
                1
            };
            Some((*r, id))
        })
        .collect();
    let after_lists = lists
        .iter()
        .map(|(repr, id)| {
            id + if matches!(*repr, Repr::MapStrInt | Repr::MapStrPtr(_)) {
                3
            } else if repr.growing() {
                2
            } else {
                1
            }
        })
        .max()
        .unwrap_or(rt_descriptors() + descriptors.len());
    out.push_str("static DESCRIPTORS: &[Descriptor] = &[rt::STRING_DESC,rt::OVERFLOW_DESC,rt::SHIFT_DESC,rt::CANCEL_DESC,rt::STACK_DESC,rt::ALLOC_DESC,rt::INDEX_DESC,rt::json::PARSE_DESC,rt::json::DECODE_DESC,rt::DIV_DESC,rt::strings::CONCAT_DESC,rt::strings::ASCII_APPEND_DESC,rt::native::DESC,rt::bytes::DESC,\n");
    for (&object, layout) in &plan.layouts {
        let class = plan.class(object);
        let fields = class
            .fields
            .iter()
            .enumerate()
            .map(|(i, f)| {
                format!(
                    "Field{{name:{:?},offset:{},slot:SlotKind::{},type_id:{}}}",
                    f.name,
                    layout.offsets[i],
                    if layout.fields[i].pointer() {
                        "Pointer"
                    } else if matches!(layout.fields[i], Repr::Dyn(_) | Repr::Tagged) {
                        "Tagged"
                    } else {
                        "Scalar"
                    },
                    match layout.fields[i] {
                        Repr::Ptr(c) => descriptors[&c] + 3,
                        r @ (Repr::ListInt
                        | Repr::ListPtr(_)
                        | Repr::ListStr
                        | Repr::ListListStr
                        | Repr::MapStrInt
                        | Repr::MapStrPtr(_)) => lists[&r] + 3,
                        Repr::Tagged => after_lists + 3,
                        Repr::Dyn(i) => after_lists + i + 4,
                        Repr::Str => 3,
                        Repr::Opaque => 15,
                        Repr::Bytes => 16,
                        Repr::Int => 0,
                        Repr::Float => u32::MAX as usize - 1,
                        Repr::Bool => 1,
                        Repr::Enum(e) =>
                            after_lists + 4 + plan.enums.iter().position(|i| *i == e).unwrap(),
                        Repr::Naming => after_lists + 3,
                        Repr::Null => 2,
                        Repr::Iter | Repr::Type(_) => unreachable!("null/type fields are rejected"),
                    }
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        writeln!(&mut out,"Descriptor{{kind:Kind::Class,size:{},ptr_slots:(8,{}),tagged_slots:({},{}),elem:None,type_id:{},fields:&[{fields}],finalizer:None}},",layout.bytes,layout.pointers,8+layout.pointers*8,layout.tagged,descriptors[&object]+3).unwrap();
    }
    for (repr, desc) in &lists {
        if matches!(*repr, Repr::MapStrInt | Repr::MapStrPtr(_)) {
            for i in 0..3 {
                let module = if matches!(*repr, Repr::MapStrPtr(_)) {
                    "regex_map"
                } else {
                    "maps"
                };
                writeln!(&mut out, "rt::{module}::descriptors({desc})[{i}],").unwrap();
            }
            continue;
        }
        if repr.growing() {
            let slot = Emitter::element(*repr)?.slot()?;
            writeln!(
                &mut out,
                "rt::lists::descriptor({}),rt::lists::buffer_descriptor::<{slot}>({}),",
                desc + 3,
                desc + 4
            )
            .unwrap();
            continue;
        }
        let elem = if *repr == Repr::ListInt {
            "Scalar"
        } else {
            "Pointer"
        };
        writeln!(&mut out, "Descriptor{{kind:Kind::List,size:8,ptr_slots:(0,0),tagged_slots:(0,0),elem:Some(baml_rt_gc::Elements{{offset:16,layout:baml_rt_gc::ElementLayout::{elem}}}),type_id:{},fields:&[],finalizer:None}},", desc+3).unwrap();
    }
    out.push_str("];\n");
    out.push_str(&capture_codegen::emit(
        plan,
        &descriptors,
        &lists,
        after_lists,
    )?);
    for (&object, layout) in &plan.layouts {
        write!(
            &mut out,
            "#[repr(C)] struct C{object}{{header:baml_rt_gc::Header,"
        )
        .unwrap();
        let mut ordered: Vec<_> = (0..layout.fields.len()).collect();
        ordered.sort_by_key(|i| layout.offsets[*i]);
        for i in ordered {
            write!(
                &mut out,
                "f{i}:{},",
                if layout.fields[i] == Repr::Bool {
                    "u64"
                } else {
                    layout.fields[i].rust()
                }
            )
            .unwrap();
        }
        out.push_str("}\n");
    }
    Ok(LayoutIds { descriptors, lists })
}

pub(super) struct LayoutIds {
    pub descriptors: BTreeMap<usize, usize>,
    pub lists: BTreeMap<Repr, usize>,
}
pub(super) fn rt_descriptors() -> usize {
    14
}

// A recursive alias's unfolded union can name containers whose concrete
// element representation is chosen by the producing expression. Merely
// carrying that union in a tagged local does not instantiate such a buffer.
// Actual container construction and operations still require their layout.
fn recursive_container<N: Clone>(ty: &baml_type::RuntimeTy<N>) -> bool {
    fn alias<N: Clone>(ty: &baml_type::RuntimeTy<N>) -> bool {
        use baml_type::RuntimeTy as T;
        match ty {
            T::TypeAlias(_) => true,
            T::List(t) => alias(t),
            T::Map { key, value } => alias(key) || alias(value),
            T::Union(m) => m.iter().any(alias),
            _ => false,
        }
    }
    matches!(
        ty,
        baml_type::RuntimeTy::List(_) | baml_type::RuntimeTy::Map { .. }
    ) && alias(ty)
}
