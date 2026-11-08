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
        if let RuntimeTy::List(element) = ty
            && matches!(element.as_ref(), RuntimeTy::Function { .. })
        {
            let (id, ty) = self.closure_type(element)?;
            self.closure_lists.insert(id, ty);
        }
        if let RuntimeTy::List(element) = ty {
            self.record_ty(element)?;
            self.record(self.repr(ty)?)?;
        } else if let RuntimeTy::Union(members) = ty {
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
        if let bex_vm_types::RuntimeTy::List(element) = ty
            && matches!(element.as_ref(), bex_vm_types::RuntimeTy::Function { .. })
        {
            let (id, ty) = self.closure_field_type(element)?;
            self.closure_lists.insert(id, ty);
        }
        if let bex_vm_types::RuntimeTy::List(element) = ty {
            self.record_field_ty(element)?;
            self.record(self.field_repr(ty)?)?;
        } else if let bex_vm_types::RuntimeTy::Union(members) = ty {
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
            Repr::Closure | Repr::Cell => self.environments = true,
            Repr::Ptr(c) => self.add(c)?,
            Repr::List(id) => {
                self.lists.insert(repr);
                let element = self.list_elements.borrow()[id].clone();
                self.record(element.repr)?;
                for member in element.members {
                    self.record(member)?;
                }
            }
            Repr::ListPtr(c) => {
                self.lists.insert(repr);
                self.add(c)?;
            }
            Repr::Map(_) => {
                self.lists.insert(repr);
                let (key, value) = self.map_parts(repr)?;
                if maps::callback_key(key) {
                    self.add(maps::panic_class(self)?)?;
                }
                self.record(key)?;
                self.record(value)?;
                for keys in [false, true] {
                    let snapshot = self.map_snapshot_repr(repr, keys)?;
                    self.record(snapshot)?;
                }
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
            Repr::ListClosure(_) => {
                self.environments = true;
                self.lists.insert(repr);
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
            RuntimeTy::Int | RuntimeTy::Literal(baml_type::Literal::Int(_), _) => Repr::Int,
            RuntimeTy::Float | RuntimeTy::Literal(baml_type::Literal::Float(_), _) => Repr::Float,
            RuntimeTy::Bool | RuntimeTy::Literal(baml_type::Literal::Bool(_), _) => Repr::Bool,
            RuntimeTy::String | RuntimeTy::Literal(baml_type::Literal::String(_), _) => Repr::Str,
            RuntimeTy::Unknown => Repr::Tagged,
            RuntimeTy::Function { .. } => Repr::Closure,
            RuntimeTy::Map { key, value } => self.map_repr(
                if matches!(key.as_ref(), RuntimeTy::Union(_)) {
                    Repr::Tagged
                } else {
                    self.repr(key)?
                },
                self.repr(value)?,
                capture_codegen::declared_type(self, key)?,
                capture_codegen::declared_type(self, value)?,
                capture_codegen::type_identity(self, key)?,
                capture_codegen::type_identity(self, value)?,
            ),
            RuntimeTy::Null => Repr::Null,
            RuntimeTy::List(element) => {
                if matches!(element.as_ref(), RuntimeTy::Function { .. }) {
                    Repr::ListClosure(self.closure_type(element)?.0)
                } else if let RuntimeTy::Union(members) = element.as_ref() {
                    self.list_union_repr(
                        self.repr(element)?,
                        members
                            .iter()
                            .map(|m| self.repr(m))
                            .collect::<Gen<Vec<_>>>()?,
                    )
                } else {
                    if matches!(element.as_ref(), RuntimeTy::Null) {
                        self.list_union_repr(Repr::Tagged, vec![Repr::Null])
                    } else {
                        self.list_repr(self.repr(element)?)
                    }
                }
            }
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
            T::Function { .. } => Repr::Closure,
            T::Int | T::Literal(baml_type::Literal::Int(_), _) => Repr::Int,
            T::Float | T::Literal(baml_type::Literal::Float(_), _) => Repr::Float,
            T::Bool | T::Literal(baml_type::Literal::Bool(_), _) => Repr::Bool,
            T::String | T::Literal(baml_type::Literal::String(_), _) => Repr::Str,
            T::Map { key, value } => self.map_repr(
                if matches!(key.as_ref(), T::Union(_)) {
                    Repr::Tagged
                } else {
                    self.field_repr(key)?
                },
                self.field_repr(value)?,
                capture_codegen::declared_field_type(self, key)?,
                capture_codegen::declared_field_type(self, value)?,
                capture_codegen::field_type_identity(self, key)?,
                capture_codegen::field_type_identity(self, value)?,
            ),
            T::List(element) => {
                if matches!(element.as_ref(), T::Function { .. }) {
                    Repr::ListClosure(self.closure_field_type(element)?.0)
                } else if let T::Union(members) = element.as_ref() {
                    self.list_union_repr(
                        self.field_repr(element)?,
                        members
                            .iter()
                            .map(|m| self.field_repr(m))
                            .collect::<Gen<Vec<_>>>()?,
                    )
                } else {
                    if matches!(element.as_ref(), T::Null) {
                        self.list_union_repr(Repr::Tagged, vec![Repr::Null])
                    } else {
                        self.list_repr(self.field_repr(element)?)
                    }
                }
            }
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
    let mut next_class = rt_descriptors();
    let descriptors: BTreeMap<_, _> = plan
        .layouts
        .keys()
        .map(|c| {
            let id = plan.list_error_descriptor(*c).unwrap_or_else(|| {
                let id = next_class;
                next_class += 1;
                id
            });
            (*c, id)
        })
        .collect();
    let lists: BTreeMap<_, _> = plan
        .lists
        .iter()
        .scan(next_class, |next, r| {
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
        .unwrap_or(next_class);
    let after_descriptors = after_lists
        + if plan.closure_descriptors().is_some() {
            2
        } else {
            0
        };
    out.push_str("static DESCRIPTORS: &[Descriptor] = &[rt::STRING_DESC,rt::OVERFLOW_DESC,rt::SHIFT_DESC,rt::CANCEL_DESC,rt::STACK_DESC,rt::ALLOC_DESC,rt::INDEX_DESC,rt::json::PARSE_DESC,rt::json::DECODE_DESC,rt::DIV_DESC,rt::list_errors::SERIALIZATION_DESC,rt::list_errors::INVALID_ARGUMENT_DESC,rt::MAP_KEY_DESC,\n");
    for (&object, layout) in &plan.layouts {
        if plan.list_error_descriptor(object).is_some() {
            continue;
        }
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
                        r @ (Repr::List(_)
                        | Repr::ListInt
                        | Repr::ListPtr(_)
                        | Repr::ListStr
                        | Repr::ListListStr
                        | Repr::ListClosure(_)
                        | Repr::MapStrInt
                        | Repr::Map(_)) => lists[&r] + 3,
                        Repr::Tagged => after_descriptors + 3,
                        Repr::Dyn(i) => after_descriptors + i + 4,
                        Repr::Str => 3,
                        Repr::Cell => plan.closure_descriptors().unwrap().0 + 3,
                        Repr::Closure => plan.closure_descriptors().unwrap().1 + 3,
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
            let slot = plan.element(*repr)?.slot()?;
            writeln!(
                &mut out,
                "rt::lists::descriptor::<{slot}>({}),rt::lists::buffer_descriptor::<{slot}>({}),",
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
    if let Some((cell, closure)) = plan.closure_descriptors() {
        debug_assert_eq!(cell, after_lists);
        writeln!(
            &mut out,
            "rt::closures::cell_descriptor({}),rt::closures::closure_descriptor({}),",
            cell + 3,
            closure + 3
        )
        .unwrap();
    }
    out.push_str("];\n");
    out.push_str(&capture_codegen::emit(
        plan,
        &descriptors,
        &lists,
        after_descriptors,
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
    13
}

impl Plan<'_> {
    // Feature descriptors follow the ordinary typed layouts. Constructors take
    // their ids explicitly so other features can extend the runtime prefix.
    pub(super) fn closure_descriptors(&self) -> Option<(usize, usize)> {
        if !self.environments && self.callables.is_empty() {
            return None;
        }
        let lists = self
            .lists
            .iter()
            .map(|repr| {
                if matches!(repr, Repr::MapStrInt | Repr::Map(_)) {
                    4
                } else if repr.growing() {
                    2
                } else {
                    1
                }
            })
            .sum::<usize>();
        let cell = rt_descriptors()
            + self
                .layouts
                .keys()
                .filter(|c| self.list_error_descriptor(**c).is_none())
                .count()
            + lists;
        Some((cell, cell + 1))
    }
}
