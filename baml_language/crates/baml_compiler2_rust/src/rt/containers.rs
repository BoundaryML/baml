use super::*;

impl Plan<'_> {
    pub(super) fn list_repr(&self, element: Repr) -> Repr {
        match element {
            Repr::Int => Repr::ListInt,
            Repr::Str => Repr::ListStr,
            Repr::ListStr => Repr::ListListStr,
            Repr::Ptr(c) => Repr::ListPtr(c),
            _ => {
                let mut elements = self.list_elements.borrow_mut();
                let id = elements
                    .iter()
                    .position(|r| r.repr == element && r.members.is_empty() && r.declared.is_none())
                    .unwrap_or_else(|| {
                        let id = elements.len();
                        elements.push(ListElement {
                            repr: element,
                            members: Vec::new(),
                            declared: None,
                        });
                        id
                    });
                Repr::List(id)
            }
        }
    }
    pub(super) fn list_union_repr(&self, repr: Repr, members: Vec<Repr>) -> Repr {
        let key = ListElement {
            repr,
            members,
            declared: None,
        };
        let mut elements = self.list_elements.borrow_mut();
        let id = elements
            .iter()
            .position(|r| {
                r.repr == key.repr
                    && r.declared == key.declared
                    && r.members.len() == key.members.len()
                    && r.members.iter().all(|m| key.members.contains(m))
            })
            .unwrap_or_else(|| {
                let id = elements.len();
                elements.push(key);
                id
            });
        Repr::List(id)
    }
    pub(super) fn element(&self, repr: Repr) -> Gen<Repr> {
        match repr {
            Repr::ListInt => Ok(Repr::Int),
            Repr::ListStr => Ok(Repr::Str),
            Repr::ListClosure(_) => Ok(Repr::Closure),
            Repr::ListListStr => Ok(Repr::ListStr),
            Repr::ListPtr(c) => Ok(Repr::Ptr(c)),
            Repr::List(id) => Ok(self.list_elements.borrow()[id].repr),
            _ => Err(no("indexed non-list")),
        }
    }
}

pub(super) struct Feature;
impl registry::Feature for Feature {
    fn call(
        &self,
        emitter: &Emitter<'_, '_>,
        name: &str,
        types: &[Repr],
        values: &[(String, Repr)],
    ) -> Option<Gen<(String, Repr)>> {
        matches!(
            name,
            "baml.Array.push" | "baml.Array.join" | "baml.Array.length"
        )
        .then(|| emitter.container_call(name, types, values))
    }

    fn terminator(
        &self,
        emitter: &mut Emitter<'_, '_>,
        term: &Terminator<'_>,
    ) -> Option<Gen<String>> {
        (crate::analysis::array_iter_walk(
            emitter.db,
            emitter.body,
            emitter.arity,
            term,
            &emitter.iters,
        )
        .is_some()
            || crate::analysis::array_next_call(emitter.db, term, &emitter.iters).is_some())
        .then(|| emitter.iterator_terminator(term))
    }

    fn value(
        &self,
        emitter: &Emitter<'_, '_>,
        value: &Rvalue<'_>,
        expected: Repr,
    ) -> Option<Gen<(String, Repr)>> {
        (matches!(
            value,
            Rvalue::Len(_)
                | Rvalue::Aggregate {
                    kind: AggregateKind::Array,
                    ..
                }
                | Rvalue::Array(..)
        ))
        .then(|| emitter.containers_value(value, expected))
    }
}
impl Emitter<'_, '_> {
    fn containers_value(&self, r: &Rvalue<'_>, expected: Repr) -> Gen<(String, Repr)> {
        match r {
            Rvalue::Len(place) => {
                let (value, repr) = self.place(place)?;
                let element = self.plan.element(repr)?;
                let expr = if repr.growing() {
                    format!(
                        "rt::lists::List::<{}>::from_raw({value}).len(&rt::strings::Cx::current())",
                        element.slot()?
                    )
                } else {
                    format!(
                        "*(({} + 8) as *const i64)",
                        self.fallible(&format!("rt::nonnull({value})"))
                    )
                };
                Ok((expr, Repr::Int))
            }
            Rvalue::Aggregate {
                kind: AggregateKind::Array,
                fields,
            }
            | Rvalue::Array(_, fields) => {
                let element = self.plan.element(expected)?;
                let desc = self
                    .lists
                    .get(&expected)
                    .ok_or_else(|| no("unplanned list"))?;
                let mut code = String::from("{");
                for (i, v) in fields.iter().enumerate() {
                    let v = self.coerce(self.operand(v)?, element)?;
                    write!(&mut code, "let a{i}={v};").unwrap();
                }
                if expected.growing() {
                    let slot = element.slot()?;
                    let new = self.fallible(&format!(
                        "rt::lists::List::<{slot}>::with_capacity(&mut rt::strings::Cx::current(),{desc},{})", fields.len()
                    ));
                    write!(&mut code, "let p={new};").unwrap();
                    for i in 0..fields.len() {
                        let push = self.fallible(&format!(
                            "p.push(&mut rt::strings::Cx::current(),{})",
                            element.to_slot(&format!("a{i}"))?
                        ));
                        write!(&mut code, "{push};").unwrap();
                    }
                    code.push_str("p.as_raw()}");
                    return Ok((code, expected));
                }
                let alloc =
                    self.fallible(&format!("rt::try_alloc({desc},{})", 16 + fields.len() * 8));
                write!(
                    &mut code,
                    "let p={alloc};p.add(8).cast::<usize>().write({});",
                    fields.len()
                )
                .unwrap();
                for i in 0..fields.len() {
                    if element.pointer() {
                        write!(
                            &mut code,
                            "baml_rt_gc::write_ptr(p,p.add({}).cast(),a{i});",
                            16 + i * 8
                        )
                        .unwrap();
                    } else {
                        write!(
                            &mut code,
                            "p.add({}).cast::<i64>().write(a{i});",
                            16 + i * 8
                        )
                        .unwrap();
                    }
                }
                code.push_str("p as usize}");
                Ok((code, expected))
            }
            _ => unreachable!("feature value guard"),
        }
    }
}

impl Emitter<'_, '_> {
    fn iterator_terminator(&self, term: &Terminator<'_>) -> Gen<String> {
        let mut out = String::new();
        if let Some((iter, array)) =
            crate::analysis::array_iter_walk(self.db, self.body, self.arity, term, &self.iters)
        {
            writeln!(&mut out, "l{iter}=l{array};iter_index{iter}=0;").unwrap();
            return Ok(out);
        }
        if let Some(iter) = crate::analysis::array_next_call(self.db, term, &self.iters) {
            let array = self.iter_arrays[&iter];
            let repr = self.local(array)?;
            let element = self.plan.element(repr)?;
            let (len, get) = if repr.growing() {
                let slot = element.slot()?;
                let list = format!("rt::lists::List::<{slot}>::from_raw(l{iter})");
                (
                    format!("{list}.len(&rt::strings::Cx::current())"),
                    self.fallible(&format!(
                        "{list}.get(&rt::strings::Cx::current(),iter_index{iter}) .map(|v|{})",
                        element.from_slot("v")
                    )),
                )
            } else {
                (
                    format!("*((l{iter}+8) as *const i64)"),
                    format!(
                        "*((l{iter}+16+iter_index{iter} as usize*8) as *const {})",
                        element.rust()
                    ),
                )
            };
            let value = self.coerce((get, element), Repr::Tagged)?;
            let (destination, _) = crate::analysis::continuation(term).unwrap();
            let expr = format!(
                "if iter_index{iter}>={len}{{rt::Dyn{{tag:u64::MAX,payload:0}}}}else{{let value={value};iter_index{iter}+=1;value}}"
            );
            out.push_str(&self.assign(destination, (expr, Repr::Tagged))?);
            return Ok(out);
        }
        unreachable!("iterator guard")
    }
}

impl Emitter<'_, '_> {
    fn container_call(
        &self,
        name: &str,
        types: &[Repr],
        values: &[(String, Repr)],
    ) -> Gen<(String, Repr)> {
        Ok(
            if matches!(
                name,
                "baml.Array.push" | "baml.Array.join" | "baml.Array.length"
            ) {
                let repr = values.first().ok_or_else(|| no("list receiver"))?.1;
                if !repr.growing() {
                    return Err(no("growing list builtin receiver"));
                }
                let element = self.plan.element(repr)?;
                if types.len() != 1 {
                    return Err(no("list builtin type arguments"));
                }
                let slot = element.slot()?;
                let list = format!("rt::lists::List::<{slot}>::from_raw({})", values[0].0);
                match name {
                        "baml.Array.length" if values.len() == 1 => (format!("rt_array_length(&rt::strings::Cx::current(),{list})"), Repr::Int),
                        "baml.Array.push" if values.len() == 2 => (self.fallible(&format!("rt_array_push(&mut rt::strings::Cx::current(),{list},{})",element.to_slot(&self.coerce(values[1].clone(), element)?)?)), Repr::Int),
                        "baml.Array.join" if values.len() == 2 && element == Repr::Str && values[1].1 == Repr::Str => (self.fallible(&format!("rt_array_join(&mut rt::strings::Cx::current(),{list},rt::strings::Str::from_raw({})).map(|s|s.as_raw())",values[1].0)), Repr::Str),
                        _ => return Err(no("list builtin arguments")),
                    }
            } else {
                unreachable!("container call guard")
            },
        )
    }
}

impl Plan<'_> {
    pub(super) fn alias_list_repr(&self, declared: String) -> Repr {
        let key = ListElement {
            repr: Repr::Tagged,
            members: Vec::new(),
            declared: Some(declared),
        };
        let mut elements = self.list_elements.borrow_mut();
        let id = elements
            .iter()
            .position(|old| *old == key)
            .unwrap_or_else(|| {
                let id = elements.len();
                elements.push(key);
                id
            });
        Repr::List(id)
    }
}
