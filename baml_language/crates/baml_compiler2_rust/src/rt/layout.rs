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
                self.record_ty(member)?;
            }
        } else if let RuntimeTy::Map { key, value } = ty {
            self.record_ty(key)?;
            self.record_ty(value)?;
            self.record(self.repr(ty)?)?;
        } else if !matches!(ty, RuntimeTy::Function { .. }) {
            self.record(self.repr(ty)?)?;
        }
        Ok(())
    }
    pub(super) fn record_field_ty(&mut self, ty: &bex_vm_types::RuntimeTy) -> Gen<()> {
        if let bex_vm_types::RuntimeTy::Union(members) = ty {
            for member in members {
                self.record_field_ty(member)?;
            }
        } else if let bex_vm_types::RuntimeTy::Map { key, value } = ty {
            self.record_field_ty(key)?;
            self.record_field_ty(value)?;
            self.record(self.field_repr(ty)?)?;
        } else {
            self.record(self.field_repr(ty)?)?;
        }
        Ok(())
    }
    pub(super) fn record(&mut self, repr: Repr) -> Gen<()> {
        match repr {
            Repr::Ptr(c) => self.add(c)?,
            Repr::ListPtr(c) => {
                self.lists.insert(repr);
                self.add(c)?;
            }
            Repr::Map(_) => {
                self.lists.insert(repr);
                let (key, value) = self.map_parts(repr)?;
                self.record(key)?;
                self.record(value)?;
                for element in [key, value] {
                    if let Repr::Ptr(c) = element {
                        self.record(Repr::ListPtr(c))?;
                    }
                    if element == Repr::ListStr {
                        self.record(Repr::ListListStr)?;
                    }
                }
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
            Repr::Dyn(i) => {
                let classes: Vec<_> = self.implementors(i)?.iter().map(|(c, _)| *c).collect();
                for c in classes {
                    self.add(c)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn repr(&self, ty: &RuntimeTy) -> Gen<Repr> {
        Ok(match ty {
            RuntimeTy::Float => Repr::Float,
            RuntimeTy::Int | RuntimeTy::Literal(baml_type::Literal::Int(_), _) => Repr::Int,
            RuntimeTy::Bool | RuntimeTy::Literal(baml_type::Literal::Bool(_), _) => Repr::Bool,
            RuntimeTy::String | RuntimeTy::Literal(baml_type::Literal::String(_), _) => Repr::Str,
            RuntimeTy::Unknown => Repr::Tagged,
            RuntimeTy::Map { key, value } => {
                if matches!(key.as_ref(), RuntimeTy::Union(m) if !m.iter().all(scalar_key))
                    || matches!(key.as_ref(), RuntimeTy::Unknown)
                {
                    return Err(no("map key callbacks"));
                }
                self.map_repr(
                    if matches!(key.as_ref(), RuntimeTy::Union(_)) {
                        Repr::Tagged
                    } else {
                        self.repr(key)?
                    },
                    self.repr(value)?,
                    capture_codegen::declared_type(self, key)?,
                    capture_codegen::declared_type(self, value)?,
                    scalar_json_value(value),
                )
            }
            RuntimeTy::Null => Repr::Null,
            RuntimeTy::List(element) => match element.as_ref() {
                RuntimeTy::Int => Repr::ListInt,
                RuntimeTy::String => Repr::ListStr,
                RuntimeTy::List(t) if matches!(t.as_ref(), RuntimeTy::String) => Repr::ListListStr,
                RuntimeTy::Union(_) => match self.repr(element)? {
                    Repr::Ptr(c) => Repr::ListPtr(c),
                    _ => return Err(no("list element")),
                },
                _ => return Err(no("list element")),
            },
            RuntimeTy::Interface(head, args, assoc) if args.is_empty() && assoc.is_empty() => {
                Repr::Dyn(
                    self.index
                        .interface_at(head)
                        .ok_or_else(|| no("interface"))?,
                )
            }
            RuntimeTy::Enum(head)
                if Some(head.root()) == self.lang.get(baml_base::LangPackage::Baml)
                    && head.namespace().as_slice() == ["json"]
                    && head.name().as_str() == "Naming" =>
            {
                Repr::Naming
            }
            RuntimeTy::Class(head, args)
                if args.is_empty()
                    && head.builtin_primitive(self.lang)
                        == Some(baml_type::PrimitiveType::String) =>
            {
                Repr::Str
            }
            RuntimeTy::Class(head, args) if args.is_empty() => Repr::Ptr(
                self.index
                    .class_at(head)
                    .ok_or_else(|| no("unlinked class"))?
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
                    self.repr(member)?;
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
            T::Unknown => Repr::Tagged,
            T::Float => Repr::Float,
            T::Int | T::Literal(baml_type::Literal::Int(_), _) => Repr::Int,
            T::Bool | T::Literal(baml_type::Literal::Bool(_), _) => Repr::Bool,
            T::String | T::Literal(baml_type::Literal::String(_), _) => Repr::Str,
            T::Map { key, value } => {
                if matches!(key.as_ref(), T::Union(m) if !m.iter().all(scalar_field_key))
                    || matches!(key.as_ref(), T::Unknown)
                {
                    return Err(no("map key callbacks"));
                }
                self.map_repr(
                    if matches!(key.as_ref(), T::Union(_)) {
                        Repr::Tagged
                    } else {
                        self.field_repr(key)?
                    },
                    self.field_repr(value)?,
                    capture_codegen::declared_field_type(self, key)?,
                    capture_codegen::declared_field_type(self, value)?,
                    scalar_json_field_value(value),
                )
            }
            T::List(element) => match element.as_ref() {
                T::Int => Repr::ListInt,
                T::String => Repr::ListStr,
                T::List(t) if matches!(t.as_ref(), T::String) => Repr::ListListStr,
                T::Union(_) => match self.field_repr(element)? {
                    Repr::Ptr(c) => Repr::ListPtr(c),
                    _ => return Err(no("list field element")),
                },
                _ => return Err(no("list field element")),
            },
            T::Interface(head, args, assoc) if args.is_empty() && assoc.is_empty() => Repr::Dyn(
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
                    self.field_repr(member)?;
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
        let Object::Class(class) =
            &self.program.objects[bex_vm_types::ObjectIndex::from_raw(object)]
        else {
            return Err(no("non-class layout"));
        };
        // Runtime-produced exception descriptors are reserved. The runtime supports
        // wildcard panic exclusion and user-defined typed catches; never emit a
        // second, incompatible layout/tag for a builtin exception class.
        if self.program.packages.iter().any(|package| {
            package.name == "baml"
                && package
                    .objects
                    .contains(&u32::try_from(object).expect("object index"))
        }) && (class.path.to_string().starts_with("panics.")
            || matches!(
                class.path.to_string().as_str(),
                "json.ParseError" | "json.DecodeError"
            ))
        {
            return Err(no("typed builtin exception class"));
        }
        registry::admit(registry::Admission::Class {
            cleanup: class.cleanup.is_some(),
            generic: class.generic_param_count != 0,
        })?;
        let fields = class
            .fields
            .iter()
            .map(|f| self.field_repr(&f.field_type))
            .collect::<Gen<Vec<_>>>()?;
        if fields.contains(&Repr::Null) {
            return Err(no("null-only field"));
        }
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
        let field_types: Vec<_> = class.fields.iter().map(|f| f.field_type.clone()).collect();
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
    let program = plan.program;
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
            *next += if matches!(r, Repr::MapStrInt | Repr::Map(_)) {
                4
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
            id + if matches!(repr, Repr::MapStrInt | Repr::Map(_)) {
                4
            } else if repr.growing() {
                2
            } else {
                1
            }
        })
        .max()
        .unwrap_or(rt_descriptors() + descriptors.len());
    out.push_str("static DESCRIPTORS: &[Descriptor] = &[rt::STRING_DESC,rt::OVERFLOW_DESC,rt::SHIFT_DESC,rt::CANCEL_DESC,rt::STACK_DESC,rt::ALLOC_DESC,rt::INDEX_DESC,rt::json::PARSE_DESC,rt::json::DECODE_DESC,rt::DIV_DESC,rt::MAP_KEY_DESC,\n");
    for (&object, layout) in &plan.layouts {
        let Object::Class(class) = &program.objects[bex_vm_types::ObjectIndex::from_raw(object)]
        else {
            unreachable!()
        };
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
                        | Repr::Map(_)) => lists[&r] + 3,
                        Repr::Tagged => after_lists + 3,
                        Repr::Dyn(i) => after_lists + i + 4,
                        Repr::Str => 3,
                        Repr::Int => 0,
                        Repr::Float => u32::MAX as usize - 1,
                        Repr::Bool => 1,
                        Repr::Iter | Repr::Naming | Repr::Null | Repr::Type(_) =>
                            unreachable!("null/type fields are rejected"),
                    }
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        writeln!(&mut out,"Descriptor{{kind:Kind::Class,size:{},ptr_slots:(8,{}),tagged_slots:({},{}),elem:None,type_id:{},fields:&[{fields}],finalizer:None}},",layout.bytes,layout.pointers,8+layout.pointers*8,layout.tagged,descriptors[&object]+3).unwrap();
    }
    for (repr, desc) in &lists {
        if let Repr::Map(_) = repr {
            let (key, value) = plan.map_parts(*repr)?;
            let key = maps::slot_type(key, &descriptors, &lists)?;
            let value = maps::slot_type(value, &descriptors, &lists)?;
            for i in 0..4 {
                writeln!(
                    out,
                    "rt::maps::typed_descriptors::<{key},{value}>({desc})[{i}],"
                )
                .unwrap();
            }
            continue;
        }
        if *repr == Repr::MapStrInt {
            for i in 0..4 {
                writeln!(&mut out, "rt::maps::descriptors({desc})[{i}],").unwrap();
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
    11
}

fn scalar_key(ty: &RuntimeTy) -> bool {
    matches!(
        ty,
        RuntimeTy::Int
            | RuntimeTy::Bool
            | RuntimeTy::Float
            | RuntimeTy::Null
            | RuntimeTy::String
            | RuntimeTy::Literal(_, _)
    )
}
fn scalar_field_key(ty: &bex_vm_types::RuntimeTy) -> bool {
    use bex_vm_types::RuntimeTy as T;
    matches!(
        ty,
        T::Int | T::Bool | T::Float | T::Null | T::String | T::Literal(_, _)
    )
}

fn scalar_json_value(ty: &RuntimeTy) -> bool {
    if let RuntimeTy::Union(members) = ty {
        members.iter().all(scalar_json_value)
    } else {
        scalar_key(ty)
    }
}
fn scalar_json_field_value(ty: &bex_vm_types::RuntimeTy) -> bool {
    if let bex_vm_types::RuntimeTy::Union(members) = ty {
        members.iter().all(scalar_json_field_value)
    } else {
        scalar_field_key(ty)
    }
}
