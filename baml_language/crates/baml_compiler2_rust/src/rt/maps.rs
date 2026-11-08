//! Map storage, literals, indexing and builtin lowering. Lists stay in containers.
use super::*;
pub(super) struct MapType {
    pub key: Repr,
    pub value: Repr,
    pub key_type: String,
    pub value_type: String,
    key_identity: String,
    value_identity: String,
}
impl PartialEq for MapType {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
            && self.value == other.value
            && self.key_identity == other.key_identity
            && self.value_identity == other.value_identity
    }
}
impl Eq for MapType {}
pub(super) fn callback_key(key: Repr) -> bool {
    !matches!(
        key,
        Repr::Str | Repr::Int | Repr::Float | Repr::Bool | Repr::Null
    )
}
pub(super) fn panic_class(plan: &Plan<'_>) -> Gen<usize> {
    plan.program
        .packages
        .iter()
        .find(|p| p.name == "baml")
        .and_then(|p| {
            p.classes
                .iter()
                .find(|(path, _)| path.to_string() == "panics.UserPanic")
                .map(|(_, id)| id.raw())
        })
        .ok_or_else(|| no("map key panic declaration"))
}
impl Plan<'_> {
    pub(super) fn map_repr(
        &self,
        key: Repr,
        value: Repr,
        key_ty: String,
        value_ty: String,
        key_identity: String,
        value_identity: String,
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
            key_identity,
            value_identity,
        };
        let mut types = self.map_types.borrow_mut();
        let id = types.iter().position(|old| old == &ty).unwrap_or_else(|| {
            types.push(ty);
            types.len() - 1
        });
        Repr::Map(id)
    }
    pub(super) fn map_snapshot_repr(&self, map: Repr, keys: bool) -> Gen<Repr> {
        let (key, value) = self.map_parts(map)?;
        let element = if keys { key } else { value };
        let (key_type, value_type) = if let Repr::Map(id) = map {
            let types = self.map_types.borrow();
            (types[id].key_type.clone(), types[id].value_type.clone())
        } else {
            (
                capture_codegen::capture_type(self, key)?,
                capture_codegen::capture_type(self, value)?,
            )
        };
        let declared = if keys { key_type } else { value_type };
        let default = if element == Repr::Null {
            self.list_union_repr(Repr::Tagged, vec![Repr::Null])
        } else {
            self.list_repr(element)
        };
        let expected = format!("rt::telemetry::capture::Type::List(&{declared})");
        if capture_codegen::capture_type(self, default).ok().as_ref() == Some(&expected) {
            return Ok(default);
        }
        let storage = if element == Repr::Null {
            Repr::Tagged
        } else {
            element
        };
        let list = ListElement {
            repr: storage,
            members: Vec::new(),
            declared: Some(declared),
        };
        let mut types = self.list_elements.borrow_mut();
        let id = types
            .iter()
            .position(|old| *old == list)
            .unwrap_or_else(|| {
                types.push(list);
                types.len() - 1
            });
        Ok(Repr::List(id))
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
            plan.list_stringify = true;
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
        fn check(plan: &Plan<'_>, repr: Repr, seen: &mut BTreeSet<Repr>) -> Gen<()> {
            if !seen.insert(repr) {
                return Ok(());
            }
            if let Repr::Ptr(c) = repr {
                let Object::Class(class) =
                    &plan.program.objects[bex_vm_types::ObjectIndex::from_raw(c)]
                else {
                    unreachable!()
                };
                for package in &plan.program.packages {
                    for (interface, rules) in &package.impl_rules {
                        let Object::Interface(def) = &plan.program.objects[*interface] else {
                            continue;
                        };
                        if !matches!(def.path.to_string().as_str(), "Hash" | "ops.Equals") {
                            continue;
                        }
                        for rule in rules {
                            if matches!(&rule.for_ty_pattern,bex_vm_types::TyTemplate::Class(head,_) if head.tag()==class.type_tag)
                                && rule.methods.values().any(|m| {
                                    matches!(
                                        &plan.index.info(m.fqn.raw()).identity,
                                        bex_vm_types::types::FunctionIdentity::Declared(_)
                                    )
                                })
                            {
                                return Err(no("custom map hash/equality callback"));
                            }
                        }
                    }
                }
                for &field in &plan.layouts[&c].fields {
                    check(plan, field, seen)?;
                }
            } else if matches!(repr, Repr::Tagged | Repr::Dyn(_)) {
                for &class in plan.layouts.keys() {
                    check(plan, Repr::Ptr(class), seen)?;
                }
            } else if let Repr::Map(_) | Repr::MapStrInt = repr {
                let (k, v) = plan.map_parts(repr)?;
                check(plan, k, seen)?;
                check(plan, v, seen)?;
            } else if matches!(
                repr,
                Repr::ListInt | Repr::ListStr | Repr::ListListStr | Repr::ListPtr(_)
            ) {
                check(plan, plan.element(repr)?, seen)?;
            } else if matches!(repr, Repr::Closure | Repr::Cell | Repr::ListClosure(_)) {
                return Err(no("unhashable map key"));
            }
            Ok(())
        }
        check(self.plan, repr, &mut BTreeSet::new())
    }
    fn map_operation(
        &self,
        map: &str,
        key: Repr,
        method: &str,
        k: &str,
        value: Option<&str>,
    ) -> Gen<String> {
        if !callback_key(key) {
            let v = value.map(|v| format!(",{v}")).unwrap_or_default();
            return Ok(format!(
                "{map}.{method}(&mut rt::strings::Cx::current(),{k}{v})"
            ));
        }
        let desc = self.descriptors[&panic_class(self.plan)?];
        let dyn_key =
            capture_codegen::capture_dyn(self.plan, key, "key", self.descriptors, self.lists)?;
        let a = capture_codegen::capture_dyn(self.plan, key, "a", self.descriptors, self.lists)?;
        let b = capture_codegen::capture_dyn(self.plan, key, "b", self.descriptors, self.lists)?;
        let v = value.map(|v| format!(",{v}")).unwrap_or_default();
        Ok(format!(
            "{{let key={k};let mut key_cx=rt::strings::Cx::current();rt::maps::keys::hash(&mut key_cx,{dyn_key},CAPTURE_SHAPES,|cx,message|rt::maps::keys::panic(cx,{desc},message)).and_then(|hash|{map}.{method}_with(&mut key_cx,key{v},hash,|cx,a,b|rt::maps::keys::equals(cx,{a},{b},CAPTURE_SHAPES)))}}"
        ))
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
            let set = self.fallible(&self.map_operation(
                "map",
                key,
                "set",
                &self.map_in(&k, key)?,
                Some(&self.map_in(&v, value)?),
            )?);
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
                let snapshot = self.plan.map_snapshot_repr(*repr, method == "keys")?;
                self.map_snapshot(&map, element, method == "keys", snapshot)
            }
            _ => {
                self.check_map_key(key)?;
                let k = values.get(1).ok_or_else(|| no("map key argument"))?;
                let k = self.coerce(k.clone(), key)?;
                let k = self.map_in(&k, key)?;
                let v = if method == "set" || method == "get_or_insert" {
                    let v = values.get(2).ok_or_else(|| no("map value argument"))?;
                    let v = self.coerce(v.clone(), value)?;
                    Some(self.map_in(&v, value)?)
                } else {
                    None
                };
                let operation = self.map_operation(
                    &map,
                    key,
                    if method == "index" { "get" } else { method },
                    &k,
                    v.as_deref(),
                )?;
                if method == "has" {
                    return Ok((self.fallible(&operation), Repr::Bool));
                }
                if method == "index" {
                    let expr = self.fallible(&format!(
                        "{operation}.and_then(|v|v.ok_or_else(rt::map_key_not_found))"
                    ));
                    return Ok((self.map_out(&expr, value), value));
                }
                let expr = operation;
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
    fn map_snapshot(
        &self,
        map: &str,
        element: Repr,
        keys: bool,
        repr: Repr,
    ) -> Gen<(String, Repr)> {
        let desc = *self
            .lists
            .get(&repr)
            .ok_or_else(|| no("map snapshot descriptor"))?;
        let select = if keys { 0 } else { 1 };
        let mut out = format!("{{let map={map};let len=map.len();");
        if repr.growing() {
            let list_element = self.plan.element(repr)?;
            let slot = list_element.slot()?;
            let new = self.fallible(&format!(
                "rt::lists::List::<{slot}>::new(&mut rt::strings::Cx::current(),{desc})"
            ));
            let raw = self.map_out(&format!("map.entry(i).unwrap().{select}"), element);
            let item = list_element.to_slot(&self.coerce((raw, element), list_element)?)?;
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
