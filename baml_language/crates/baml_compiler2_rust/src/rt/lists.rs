//! Typed list builtins, independent of map and generic-call lowering.
use super::*;

impl Plan<'_> {
    pub(super) fn list_error_descriptor(&self, object: usize) -> Option<usize> {
        if !self
            .program
            .packages
            .iter()
            .any(|p| p.name == "baml" && p.objects.contains(&(object as u32)))
        {
            return None;
        }
        let Object::Class(class) =
            &self.program.objects[bex_vm_types::ObjectIndex::from_raw(object)]
        else {
            return None;
        };
        match class.path.to_string().as_str() {
            "panics.IntegerOverflow" => Some(1),
            "panics.NegativeBitShift" => Some(2),
            "panics.Cancelled" => Some(3),
            "panics.StackOverflow" => Some(4),
            "panics.AllocFailure" => Some(5),
            "panics.IndexOutOfBounds" => Some(6),
            "json.ParseError" => Some(7),
            "json.DecodeError" => Some(8),
            "panics.DivisionByZero" => Some(9),
            "panics.MapKeyNotFound" => Some(12),
            "json.SerializationError" => Some(10),
            "errors.InvalidArgument" => Some(11),
            _ => None,
        }
    }
}

pub(super) fn handles(name: &str) -> bool {
    matches!(
        name,
        "baml._sum_int"
            | "baml._sum_float"
            | "baml._median_float"
            | "baml._rust_sort"
            | "baml._is_primitive_array"
            | "baml.Array.push"
            | "baml.Array.length"
            | "baml.Array.join"
            | "baml.Array.at"
            | "baml.Array.pop"
            | "baml.Array.remove_at"
            | "baml.Array.concat"
            | "baml.Array.reverse"
            | "baml.Array.slice"
            | "baml.Array.clear"
            | "baml.Array.shift"
            | "baml.Array.unshift"
            | "baml.Array.insert"
            | "baml.Array.splice"
            | "baml.Array.includes"
            | "baml.Array.index_of"
            | "baml.Array.last_index_of"
    )
}
pub(super) struct Feature;
impl registry::Feature for Feature {
    fn reach(
        &self,
        db: &dyn Db,
        plan: &mut Plan<'_>,
        term: Option<&Terminator<'_>>,
    ) -> Option<Gen<registry::Reach>> {
        let Some(Terminator::Call {
            callee: Operand::Constant(Constant::Function(f)),
            has_trace: false,
            ..
        }) = term
        else {
            return None;
        };
        let (callee, _) = plan.index.function(db, *f)?;
        let name = plan.index.display(callee);
        if name == "baml.json.stringify" {
            plan.list_stringify = true;
            return Some(Ok(registry::Reach::default()));
        }
        (handles(name) || name == "baml.Array.filled").then(|| Ok(registry::Reach::default()))
    }
    fn terminator(
        &self,
        emitter: &mut Emitter<'_, '_>,
        term: &Terminator<'_>,
    ) -> Option<Gen<String>> {
        let Terminator::Call {
            callee: Operand::Constant(Constant::Function(f)),
            args,
            destination,
            argument_layout,
            ntypeargs,
            ..
        } = term
        else {
            return None;
        };
        let (callee, _) = emitter.plan.index.function(emitter.db, *f)?;
        let name = emitter.plan.index.display(callee);
        if !matches!(name, "baml.Array.filled" | "baml.json.stringify") {
            return None;
        }
        let ordered;
        let args = if let Some(layout) = argument_layout {
            let mapping = match layout.map_to(&emitter.plan.index.info(callee).argument_layout()) {
                Ok(m) => m,
                Err(e) => return Some(Err(no(e))),
            };
            let mut arguments = args[..*ntypeargs].to_vec();
            arguments.extend(mapping.into_iter().map(|i| {
                i.map(|i| args[*ntypeargs + i].clone())
                    .unwrap_or(Operand::Constant(Constant::OmittedArg))
            }));
            ordered = arguments;
            &ordered
        } else {
            args
        };
        if emitter.plan.index.display(callee) == "baml.Array.filled" {
            return Some((|| {
                let Terminator::Call { ntypeargs, .. } = term else {
                    unreachable!()
                };
                let (_, repr) = emitter.place(destination)?;
                let element = emitter.plan.element(repr)?;
                let slot = element.slot()?;
                let len = emitter.coerce(emitter.operand(&args[*ntypeargs])?, Repr::Int)?;
                let value = element
                    .to_slot(&emitter.coerce(emitter.operand(&args[*ntypeargs + 1])?, element)?)?;
                let desc = emitter.lists[&repr];
                let expression=emitter.fallible(&format!("rt::lists::List::<{slot}>::filled(&mut rt::strings::Cx::current(),{desc},{len},{value}).map(|v|v.as_raw())"));
                emitter.assign(destination, (expression, repr))
            })());
        }
        (emitter.plan.index.display(callee) == "baml.json.stringify").then(|| {
            let defaults = [Repr::Tagged, Repr::Naming, Repr::Tagged, Repr::Bool];
            let mut values = Vec::new();
            for (i, arg) in args.iter().enumerate() {
                let value = if matches!(arg, Operand::Constant(Constant::OmittedArg)) {
                    match i {
                        1 => "0usize".into(),
                        2 => "rt::Dyn{tag:0,payload:0}".into(),
                        3 => "false".into(),
                        _ => return Err(no("stringify omitted value")),
                    }
                } else {
                    emitter.coerce(emitter.operand(arg)?, defaults[i])?
                };
                values.push(value);
            }
            if values.len() != 4 {
                return Err(no("list stringify arguments"));
            }
            let omitted = args
                .iter()
                .map(|a| matches!(a, Operand::Constant(Constant::OmittedArg)))
                .collect::<Vec<_>>();
            let site = emitter.telemetry_site(callee);
            let expression = emitter.fallible(&format!(
                "f{callee}_list_json({site},{},{},{},{},{},{},{})",
                values[0], values[1], values[2], values[3], omitted[1], omitted[2], omitted[3]
            ));
            emitter.assign(destination, (expression, Repr::Str))
        })
    }
    fn call(
        &self,
        emitter: &Emitter<'_, '_>,
        name: &str,
        _types: &[Repr],
        values: &[(String, Repr)],
    ) -> Option<Gen<(String, Repr)>> {
        handles(name).then(|| emitter.list_call(name, values))
    }
}
impl Emitter<'_, '_> {
    fn list_call(&self, name: &str, values: &[(String, Repr)]) -> Gen<(String, Repr)> {
        let repr = values.first().ok_or_else(|| no("list receiver"))?.1;
        let element = self.plan.element(repr)?;
        let slot = element.slot()?;
        let receiver = &values[0].0;
        let list = format!("rt::lists::List::<{slot}>::from_raw({receiver})");
        let cx = "&mut rt::strings::Cx::current()";
        let method = name.strip_prefix("baml.Array.").unwrap_or(name);
        let optional = |expression: String| -> Gen<(String, Repr)> {
            let value = self.coerce((element.from_slot("value"), element), Repr::Tagged)?;
            Ok((
                format!(
                    "match {expression}{{Some(value)=>{value},None=>rt::Dyn{{tag:2,payload:0}}}}"
                ),
                Repr::Tagged,
            ))
        };
        match (method, values.len()) {
            ("baml._sum_int", 1) if element == Repr::Int => {
                Ok((self.fallible(&format!("{list}.sum({cx})")), Repr::Int))
            }
            ("baml._sum_float", 1) if element == Repr::Float => {
                Ok((format!("{list}.sum({cx})"), Repr::Float))
            }
            ("baml._median_float", 1) if element == Repr::Float => {
                Ok((self.fallible(&format!("{list}.median({cx})")), Repr::Float))
            }
            ("baml._is_primitive_array", 1) => Ok((
                matches!(element, Repr::Int | Repr::Float | Repr::Str).to_string(),
                Repr::Bool,
            )),
            ("baml._rust_sort", 1) => {
                if let Repr::List(id) = repr
                    && self.plan.list_elements.borrow()[id]
                        .members
                        .contains(&Repr::Null)
                {
                    return Err(no("natural sort of nullable list elements"));
                }
                let compare = match element {
                    Repr::Int => "a.cmp(&b)",
                    Repr::Float => "rt::float_cmp(a,b)",
                    Repr::Str => {
                        "rt::strings::Str::from_raw(a.as_raw()).as_str(&read_cx).cmp(rt::strings::Str::from_raw(b.as_raw()).as_str(&read_cx))"
                    }
                    _ => return Err(no("list natural-sort comparator")),
                };
                Ok((self.fallible(&format!("{{let read_cx=rt::strings::Cx::current();{list}.sort_native({cx},|a,b|{compare}).map(|v|v.as_raw())}}")),repr))
            }

            ("length", 1) => Ok((format!("{list}.len({cx})"), Repr::Int)),
            ("push" | "unshift", 2) => {
                let value = element.to_slot(&self.coerce(values[1].clone(), element)?)?;
                Ok((
                    self.fallible(&format!("{list}.{method}({cx},{value})")),
                    Repr::Int,
                ))
            }
            ("insert", 3) => {
                let value = element.to_slot(&self.coerce(values[1].clone(), element)?)?;
                let index = self.coerce(values[2].clone(), Repr::Int)?;
                Ok((
                    self.fallible(&format!("{list}.insert({cx},{value},{index})")),
                    Repr::Int,
                ))
            }
            ("splice", 4) => {
                let start = self.coerce(values[1].clone(), Repr::Int)?;
                let count = self.coerce(values[2].clone(), Repr::Int)?;
                let replace = self.coerce(values[3].clone(), repr)?;
                Ok((format!("{{{};0usize}}",self.fallible(&format!("{list}.splice({cx},{start},{count},rt::lists::List::<{slot}>::from_raw({replace}))"))),Repr::Null))
            }
            ("at" | "remove_at", 2) => {
                let index = self.coerce(values[1].clone(), Repr::Int)?;
                optional(format!("{list}.{method}({cx},{index})"))
            }
            ("pop", 1) => optional(format!("{list}.pop({cx})")),
            ("shift", 1) => optional(format!("{list}.remove_at({cx},0)")),
            ("clear", 1) => Ok((format!("{{{list}.clear({cx});0usize}}"), Repr::Null)),
            ("reverse", 1) => Ok((
                self.fallible(&format!("{list}.reverse({cx}).map(|v|v.as_raw())")),
                repr,
            )),
            ("slice", 3) => {
                let start = self.coerce(values[1].clone(), Repr::Int)?;
                let end = self.coerce(values[2].clone(), Repr::Int)?;
                Ok((
                    self.fallible(&format!(
                        "{list}.slice({cx},{start},{end}).map(|v|v.as_raw())"
                    )),
                    repr,
                ))
            }
            ("concat", 2) => {
                let other = self.coerce(values[1].clone(), repr)?;
                Ok((self.fallible(&format!("{list}.concat({cx},rt::lists::List::<{slot}>::from_raw({other})).map(|v|v.as_raw())")), repr))
            }
            ("includes" | "index_of" | "last_index_of", 2) => {
                let item = element.to_slot(&self.coerce(values[1].clone(), element)?)?;
                let equality=match element {
                    Repr::Str=>"if a.as_raw()==0 || b.as_raw()==0 {a.as_raw()==b.as_raw()} else {rt::strings::Str::from_raw(a.as_raw()).as_str(&read_cx)==rt::strings::Str::from_raw(b.as_raw()).as_str(&read_cx)}".into(),
                    Repr::Float=>"rt::float_eq(a,b)".into(),
                    Repr::Tagged|Repr::Dyn(_)=>"rt::lists::builtin_eq(&read_cx,a,b)".into(),
                    r if r.pointer()=>"a.as_raw()==b.as_raw()".into(),
                    _=>"a==b".to_owned(),
                };
                let search = format!(
                    "{{let read_cx=rt::strings::Cx::current();{list}.index_of(&read_cx,{item},{},|a,b|{equality})}}",
                    method == "last_index_of"
                );
                if method == "includes" {
                    Ok((format!("({search}).is_some()"), Repr::Bool))
                } else {
                    Ok((
                        format!(
                            "match {search}{{Some(v)=>rt::Dyn{{tag:0,payload:v as u64}},None=>rt::Dyn{{tag:2,payload:0}}}}"
                        ),
                        Repr::Tagged,
                    ))
                }
            }
            ("join", 2) if matches!(element, Repr::Int | Repr::Float | Repr::Bool) => {
                let separator = self.coerce(values[1].clone(), Repr::Str)?;
                let render = if element == Repr::Float {
                    "rt::format_float(value)"
                } else {
                    "value.to_string()"
                };
                Ok((self.fallible(&format!("rt::lists::join_with({cx},{list},rt::strings::Str::from_raw({separator}),|value|rt::strings::Cx::current().string(&{render})).map(|v|v.as_raw())")),Repr::Str))
            }
            ("join", 2) if element == Repr::Str => {
                let separator = self.coerce(values[1].clone(), Repr::Str)?;
                Ok((self.fallible(&format!("rt::lists::join_pointers({cx},{list},rt::strings::Str::from_raw({separator})).map(|v|v.as_raw())")), Repr::Str))
            }
            ("join", 2) => {
                self.plan.list_render_roots.borrow_mut().insert(repr);
                let separator = self.coerce(values[1].clone(), Repr::Str)?;
                let value = self.coerce((element.from_slot("value"), element), Repr::Tagged)?;
                Ok((self.fallible(&format!("rt::lists::join_with({cx},{list},rt::strings::Str::from_raw({separator}),|value|rt::lists::structural_string(&mut rt::strings::Cx::current(),{value},DESCRIPTORS,LIST_RENDER_SHAPES)).map(|v|v.as_raw())")),Repr::Str))
            }
            _ => Err(no(format!("list builtin {method} arguments"))),
        }
    }
}

pub(super) fn emit_render_shapes(
    out: &mut String,
    plan: &Plan<'_>,
    descriptors: &BTreeMap<usize, usize>,
    lists: &BTreeMap<Repr, usize>,
) -> Gen<()> {
    let mut pending: Vec<_> = plan.list_render_roots.borrow().iter().copied().collect();
    if pending.is_empty() {
        return Ok(());
    }
    let mut visited = BTreeSet::new();
    while let Some(repr) = pending.pop() {
        if !visited.insert(repr) {
            continue;
        }
        match repr {
            Repr::Ptr(c) => {
                let class = plan.class(c);
                for package in &plan.program.packages {
                    for rules in package.impl_rules.values() {
                        for rule in rules {
                            if rule.methods.contains_key("to_string")
                                && matches!(&rule.for_ty_pattern,bex_vm_types::TyTemplate::Class(head,_) if head.tag()==class.type_tag)
                            {
                                return Err(no(
                                    "list join ToString override requires interface dispatch",
                                ));
                            }
                        }
                    }
                }
                pending.extend(&plan.layouts[&c].fields);
            }
            Repr::Dyn(_) => pending.extend(plan.layouts.keys().copied().map(Repr::Ptr)),
            Repr::Tagged => pending.extend(plan.layouts.keys().copied().map(Repr::Ptr)),
            r if r.growing() => {
                pending.push(plan.element(r)?);
                if let Repr::List(id) = r {
                    pending.extend(&plan.list_elements.borrow()[id].members);
                }
            }
            _ => {}
        }
    }
    let last = lists
        .iter()
        .map(|(r, id)| id + if *r == Repr::MapStrInt { 3 } else { 2 })
        .max()
        .unwrap_or_else(|| {
            descriptors
                .values()
                .max()
                .map_or(rt_descriptors(), |id| id + 1)
        });
    out.push_str("static LIST_RENDER_SHAPES:&[rt::lists::RenderShape]=&[\n");
    for desc in 0..last {
        if let Some((&c, _)) = descriptors.iter().find(|(_, id)| **id == desc) {
            let class = plan.class(c);
            writeln!(
                out,
                "rt::lists::RenderShape::Class({:?}),",
                class.path.name.to_string()
            )
            .unwrap();
        } else if let Some((&r, _)) = lists.iter().find(|(_, id)| **id == desc) {
            if r == Repr::MapStrInt {
                out.push_str("rt::lists::RenderShape::StringIntMap,\n");
                continue;
            }
            let element = plan.element(r)?;
            let tag = match element {
                Repr::Int | Repr::Tagged | Repr::Dyn(_) => 0,
                Repr::Bool => 1,
                Repr::Null => 2,
                Repr::Str => 3,
                Repr::Float => (u32::MAX - 1) as usize,
                Repr::Ptr(c) => descriptors[&c] + 3,
                r if r.growing() || r == Repr::MapStrInt => lists[&r] + 3,
                _ => return Err(no("list join element representation")),
            };
            writeln!(
                out,
                "rt::lists::RenderShape::List{{tag:{tag},tagged:{}}},",
                matches!(element, Repr::Tagged | Repr::Dyn(_))
            )
            .unwrap();
        } else {
            out.push_str("rt::lists::RenderShape::Unsupported,\n");
        }
    }
    out.push_str("];\n");
    Ok(())
}
