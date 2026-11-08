//! Map storage, literals, indexing and builtin lowering. Lists stay in containers.
use super::*;
#[derive(PartialEq, Eq)]
pub(super) struct MapType {
    pub key: Repr,
    pub value: Repr,
    pub key_type: String,
    pub value_type: String,
    pub scalar_json_value: bool,
}
impl Plan<'_> {
    pub(super) fn map_repr(
        &self,
        key: Repr,
        value: Repr,
        key_ty: String,
        value_ty: String,
        scalar_json_value: bool,
    ) -> Repr {
        if key == Repr::Str
            && value == Repr::Int
            && key_ty == "rt::telemetry::capture::Type::String"
            && value_ty == "rt::telemetry::capture::Type::Int"
        {
            return Repr::MapStrInt;
        }
        let ty = MapType {
            key,
            value,
            key_type: key_ty,
            value_type: value_ty,
            scalar_json_value,
        };
        let mut types = self.map_types.borrow_mut();
        let id = types.iter().position(|old| old == &ty).unwrap_or_else(|| {
            types.push(ty);
            types.len() - 1
        });
        Repr::Map(id)
    }
    pub(super) fn map_parts(&self, repr: Repr) -> Gen<(Repr, Repr)> {
        match repr {
            Repr::MapStrInt => Ok((Repr::Str, Repr::Int)),
            Repr::Map(id) => {
                let types = self.map_types.borrow();
                Ok((types[id].key, types[id].value))
            }
            _ => Err(no("map receiver")),
        }
    }
}
pub(super) fn slot_type(
    repr: Repr,
    descriptors: &BTreeMap<usize, usize>,
    lists: &BTreeMap<Repr, usize>,
) -> Gen<String> {
    Ok(match repr {
        Repr::Int => "i64".into(),
        Repr::Float => "f64".into(),
        Repr::Null => "rt::maps::Null".into(),
        Repr::Bool => "bool".into(),
        Repr::Tagged | Repr::Dyn(_) => "rt::Dyn".into(),
        Repr::Str => "rt::strings::Str".into(),
        Repr::Closure => "rt::closures::Closure".into(),
        Repr::Ptr(c) => format!("rt::maps::Pointer<{}>", descriptors[&c] + 3),
        r if r.pointer() => format!("rt::maps::Pointer<{}>", lists[&r] + 3),
        _ => return Err(no("map slot representation")),
    })
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
        let name = plan.index.display(callee).to_owned();
        if name == "baml.json.stringify" {
            plan.map_stringify = true;
            return Some((|| {
                let error = map_json::serialization_error_class(plan)?;
                plan.add(error)?;
                Ok(registry::Reach::default())
            })());
        }
        name.starts_with("baml.Map.").then(|| {
            if !matches!(
                name.as_str(),
                "baml.Map.set"
                    | "baml.Map.get"
                    | "baml.Map.index"
                    | "baml.Map.has"
                    | "baml.Map.delete"
                    | "baml.Map.get_or_insert"
                    | "baml.Map.clear"
                    | "baml.Map.length"
                    | "baml.Map.keys"
                    | "baml.Map.values"
            ) {
                return Err(no(format!("map builtin {name}")));
            }
            plan.record(Repr::ListInt)?;
            plan.record(Repr::ListStr)?;
            Ok(registry::Reach::default())
        })
    }
    fn terminator(
        &self,
        emitter: &mut Emitter<'_, '_>,
        term: &Terminator<'_>,
    ) -> Option<Gen<String>> {
        super::map_json::terminator(emitter, term)
    }
    fn call(
        &self,
        emitter: &Emitter<'_, '_>,
        name: &str,
        _types: &[Repr],
        values: &[(String, Repr)],
    ) -> Option<Gen<(String, Repr)>> {
        name.starts_with("baml.Map.")
            .then(|| emitter.map_call(name, values))
    }
    fn value(
        &self,
        emitter: &Emitter<'_, '_>,
        value: &Rvalue<'_>,
        expected: Repr,
    ) -> Option<Gen<(String, Repr)>> {
        if let Rvalue::Map(_, _, entries) = value {
            Some(emitter.map_literal(entries, expected))
        } else if let Rvalue::Len(place) = value {
            match emitter.place(place) {
                Ok((value, repr @ (Repr::Map(_) | Repr::MapStrInt))) => Some(
                    emitter
                        .map_type(repr)
                        .map(|ty| (format!("{ty}::from_raw({value}).len() as i64"), Repr::Int)),
                ),
                _ => None,
            }
        } else {
            None
        }
    }
}
impl Emitter<'_, '_> {
    fn map_type(&self, repr: Repr) -> Gen<String> {
        let (key, value) = self.plan.map_parts(repr)?;
        Ok(format!(
            "rt::maps::Map::<{},{}>",
            slot_type(key, self.descriptors, self.lists)?,
            slot_type(value, self.descriptors, self.lists)?
        ))
    }
    fn map_in(&self, value: &str, repr: Repr) -> Gen<String> {
        Ok(match repr {
            Repr::Str => format!("rt::strings::Str::from_raw({value})"),
            Repr::Closure => format!("rt::closures::Closure::from_raw({value})"),
            Repr::Null => format!("{{let _={value};rt::maps::Null}}"),
            Repr::Int | Repr::Float | Repr::Bool | Repr::Tagged | Repr::Dyn(_) => value.to_owned(),
            r if r.pointer() => format!("rt::maps::Pointer::<{}>::from_raw({value})", self.tag(r)?),
            _ => return Err(no("map input representation")),
        })
    }
    fn map_out(&self, value: &str, repr: Repr) -> String {
        match repr {
            Repr::Str => format!("({value}).as_raw()"),
            Repr::Null => format!("{{let _={value};0usize}}"),
            r if r.pointer() => format!("({value}).as_raw()"),
            _ => value.to_owned(),
        }
    }
    fn map_optional(&self, expr: &str, repr: Repr) -> Gen<String> {
        let value = self.coerce((self.map_out("v", repr), repr), Repr::Tagged)?;
        Ok(format!(
            "({expr}).map(|v|v.map_or(rt::Dyn{{tag:2,payload:0}},|v|{value}))"
        ))
    }
    fn check_map_key(&self, repr: Repr) -> Gen<()> {
        if matches!(
            repr,
            Repr::Str | Repr::Int | Repr::Float | Repr::Bool | Repr::Null | Repr::Tagged
        ) {
            Ok(())
        } else {
            Err(no("map key callbacks"))
        }
    }
    fn map_literal(
        &self,
        entries: &[(Operand<'_>, Operand<'_>)],
        repr: Repr,
    ) -> Gen<(String, Repr)> {
        let (key, value) = self.plan.map_parts(repr)?;
        self.check_map_key(key)?;
        let ty = self.map_type(repr)?;
        let desc = self.lists[&repr];
        let new = self.fallible(&format!(
            "{ty}::new(&mut rt::strings::Cx::current(),{desc})"
        ));
        let mut out = format!("{{let map={new};");
        for (k, v) in entries {
            let k = self.coerce(self.operand(k)?, key)?;
            let v = self.coerce(self.operand(v)?, value)?;
            let set = self.fallible(&format!(
                "map.set(&mut rt::strings::Cx::current(),{}, {})",
                self.map_in(&k, key)?,
                self.map_in(&v, value)?
            ));
            write!(out, "{set};").unwrap();
        }
        out.push_str("map.as_raw()}");
        Ok((out, repr))
    }
    fn map_call(&self, name: &str, values: &[(String, Repr)]) -> Gen<(String, Repr)> {
        let (receiver, repr) = values.first().ok_or_else(|| no("map receiver"))?;
        let (key, value) = self.plan.map_parts(*repr)?;
        let ty = self.map_type(*repr)?;
        let map = format!("{ty}::from_raw({receiver})");
        let method = name.strip_prefix("baml.Map.").unwrap();
        match method {
            "length" => Ok((format!("{map}.len() as i64"), Repr::Int)),
            "clear" => Ok((format!("{{{map}.clear();0usize}}"), Repr::Null)),
            "keys" | "values" => {
                let element = if method == "keys" { key } else { value };
                // Until the list feature carries union element metadata, do
                // not publish a nullable-class snapshot as a non-null list.
                if let Repr::Map(id) = repr {
                    let declared = {
                        let types = self.plan.map_types.borrow();
                        if method == "keys" {
                            types[*id].key_type.clone()
                        } else {
                            types[*id].value_type.clone()
                        }
                    };
                    if declared != capture_codegen::capture_type(self.plan, element)? {
                        return Err(no("map snapshot declared list element type"));
                    }
                }
                self.map_snapshot(&map, element, method == "keys")
            }
            _ => {
                self.check_map_key(key)?;
                let k = values.get(1).ok_or_else(|| no("map key argument"))?;
                let k = self.coerce(k.clone(), key)?;
                let k = self.map_in(&k, key)?;
                let mut arguments = format!("&mut rt::strings::Cx::current(),{k}");
                if method == "set" || method == "get_or_insert" {
                    let v = values.get(2).ok_or_else(|| no("map value argument"))?;
                    let v = self.coerce(v.clone(), value)?;
                    write!(arguments, ",{}", self.map_in(&v, value)?).unwrap();
                }
                if method == "has" {
                    return Ok((
                        self.fallible(&format!("{map}.has({arguments})")),
                        Repr::Bool,
                    ));
                }
                if method == "index" {
                    let expr = self.fallible(&format!(
                        "{map}.get({arguments}).and_then(|v|v.ok_or_else(rt::map_key_not_found))"
                    ));
                    return Ok((self.map_out(&expr, value), value));
                }
                let expr = format!("{map}.{method}({arguments})");
                if method == "get_or_insert" {
                    let expr = self.fallible(&expr);
                    return Ok((self.map_out(&expr, value), value));
                }
                Ok((
                    self.fallible(&self.map_optional(&expr, value)?),
                    Repr::Tagged,
                ))
            }
        }
    }
    fn map_snapshot(&self, map: &str, element: Repr, keys: bool) -> Gen<(String, Repr)> {
        let repr = match element {
            Repr::Int => Repr::ListInt,
            Repr::Str => Repr::ListStr,
            Repr::ListStr => Repr::ListListStr,
            Repr::Ptr(c) => Repr::ListPtr(c),
            _ => return Err(no("map snapshot list representation")),
        };
        let desc = *self
            .lists
            .get(&repr)
            .ok_or_else(|| no("map snapshot descriptor"))?;
        let select = if keys { 0 } else { 1 };
        let mut out = format!("{{let map={map};let len=map.len();");
        if repr.growing() {
            let slot = element.slot()?;
            let new = self.fallible(&format!(
                "rt::lists::List::<{slot}>::new(&mut rt::strings::Cx::current(),{desc})"
            ));
            let item = if element == Repr::Str {
                format!("map.entry(i).unwrap().{select}")
            } else {
                format!(
                    "<{slot}>::from_raw({})",
                    self.map_out(&format!("map.entry(i).unwrap().{select}"), element)
                )
            };
            let push = self.fallible(&format!(
                "list.push(&mut rt::strings::Cx::current(),{item})"
            ));
            write!(
                out,
                "let list={new};for i in 0..len{{{push};}}list.as_raw()}}"
            )
            .unwrap();
        } else {
            let new = self.fallible(&format!("rt::try_alloc({desc},16+len*8)"));
            let value = self.map_out(&format!("map.entry(i).unwrap().{select}"), element);
            let store = if element.pointer() {
                format!("baml_rt_gc::write_ptr(list,list.add(16+i*8).cast(),{value});")
            } else {
                format!("list.add(16+i*8).cast::<i64>().write({value});")
            };
            write!(out,"let list={new};list.add(8).cast::<usize>().write(len);for i in 0..len{{{store}}}list as usize}}").unwrap();
        }
        Ok((out, repr))
    }
    pub(super) fn map_place(&self, p: &Place) -> Option<Gen<(String, Repr)>> {
        let Place::Index {
            base,
            index,
            kind: baml_compiler2_mir::IndexKind::Map,
        } = p
        else {
            return None;
        };
        Some((|| {
            let receiver = self.place(base)?;
            let key = (format!("l{}", index.0), self.local(index.0)?);
            self.map_call("baml.Map.index", &[receiver, key])
        })())
    }
    pub(super) fn map_assign(&self, p: &Place, value: (String, Repr)) -> Option<Gen<String>> {
        let Place::Index {
            base,
            index,
            kind: baml_compiler2_mir::IndexKind::Map,
        } = p
        else {
            return None;
        };
        Some((|| {
            let receiver = self.place(base)?;
            let key = (format!("l{}", index.0), self.local(index.0)?);
            let (set, _) = self.map_call("baml.Map.set", &[receiver, key, value])?;
            Ok(format!("{set};\n"))
        })())
    }
}
