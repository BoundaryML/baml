//! Reachability and lowering for user map-key methods and their hash state.
use super::*;
use baml_compiler2_mir::TyTemplateInterface;

pub(super) fn hasher_type(plan: &Plan<'_>) -> RuntimeTy {
    RuntimeTy::Class(
        baml_type::DeclName::qualified(
            plan.lang.get(baml_base::LangPackage::Baml).unwrap(),
            vec![baml_base::Name::new("hash")],
            baml_base::Name::new("DefaultHasher"),
        ),
        Box::new([]),
    )
}
fn interface(plan: &Plan<'_>, path: &str) -> TyTemplateInterface {
    let mut segments = path.split('.').collect::<Vec<_>>();
    let name = baml_base::Name::new(segments.pop().unwrap());
    TyTemplateInterface {
        name: baml_type::DeclName::qualified(
            plan.lang.get(baml_base::LangPackage::Baml).unwrap(),
            segments.into_iter().map(baml_base::Name::new).collect(),
            name,
        ),
        generics: Box::new([]),
        associated_types: Box::new([]),
    }
}
pub(super) fn virtual_method(
    plan: &Plan<'_>,
    term: &Terminator<'_>,
) -> Option<(&'static str, String)> {
    let Terminator::VirtualCall { iface, method, .. } = term else {
        return None;
    };
    if Some(iface.name.root()) != plan.lang.get(baml_base::LangPackage::Baml) {
        return None;
    }
    let path = iface
        .name
        .namespace()
        .iter()
        .map(|n| n.as_str())
        .chain(std::iter::once(iface.name.name().as_str()))
        .collect::<Vec<_>>()
        .join(".");
    match (path.as_str(), method.as_str()) {
        ("Hash", "hash") => Some(("hash", path)),
        ("hash.Hasher", "write") => Some(("write", path)),
        ("hash.Hasher", "finish") => Some(("finish", path)),
        _ => None,
    }
}
pub(super) type Discovery = (Repr, String, usize, Vec<RuntimeTy>);
pub(super) fn discover(
    db: &dyn Db,
    plan: &mut Plan<'_>,
    body: &MirFunctionBody<'_>,
) -> Gen<Vec<Discovery>> {
    fn walk(
        plan: &mut Plan<'_>,
        r: Repr,
        seen: &mut BTreeSet<(Repr, bool)>,
        keys: &mut BTreeSet<Repr>,
        key: bool,
    ) -> Gen<()> {
        if key {
            keys.insert(r);
        }
        if !seen.insert((r, key)) {
            return Ok(());
        }
        match r {
            Repr::Map(_) | Repr::MapStrInt => {
                let (k, v) = plan.map_parts(r)?;
                walk(plan, k, seen, keys, true)?;
                walk(plan, v, seen, keys, false)?;
            }
            Repr::Ptr(c) => {
                plan.add(c)?;
                for child in plan.layouts[&c].fields.clone() {
                    walk(plan, child, seen, keys, key)?;
                }
            }
            r if r.growing() => walk(plan, plan.element(r)?, seen, keys, key)?,
            Repr::Tagged | Repr::Dyn(_) if key => {
                for ty in plan.concrete_types.clone() {
                    if let Ok(r) = plan.repr(&ty) {
                        if !matches!(r, Repr::Tagged | Repr::Dyn(_)) {
                            walk(plan, r, seen, keys, true)?;
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    let mut keys = BTreeSet::new();
    let mut seen = BTreeSet::new();
    for l in &body.locals {
        if let Ok(r) = plan.repr(&l.ty) {
            walk(plan, r, &mut seen, &mut keys, false)?;
        }
    }
    let mut result = Vec::new();
    for repr in keys {
        for (path, method) in [("Hash", "hash"), ("ops.Equals", "eq")] {
            if plan
                .key_callbacks
                .get(&repr)
                .is_some_and(|m| m.contains_key(method))
            {
                continue;
            }
            let types = plan.concrete_types.clone();
            let Some(concrete) = types.iter().find(|t| plan.repr(t).ok() == Some(repr)) else {
                continue;
            };
            if let Some((target, frame)) =
                dispatch::resolve(db, plan, concrete, &interface(plan, path), method, &[])?
            {
                if matches!(
                    plan.index.info(target).identity,
                    bex_vm_types::types::FunctionIdentity::InterfaceBody(
                        bex_vm_types::types::BodyKey::ImplMethod(_)
                    )
                ) && matches!(
                    plan.index.info(target).kind,
                    bex_vm_types::types::FunctionKind::Bytecode
                ) && !plan
                    .program
                    .packages
                    .iter()
                    .any(|p| p.name == "baml" && p.objects.contains(&(target as u32)))
                {
                    result.push((repr, method.to_owned(), target, frame));
                }
            }
        }
    }
    if !result.is_empty() {
        let ty = hasher_type(plan);
        plan.remember_heads(&ty);
        if !plan.concrete_types.contains(&ty) {
            plan.concrete_types.push(ty.clone());
        }
        let Repr::Ptr(c) = plan.repr(&ty)? else {
            unreachable!()
        };
        plan.add(c)?;
        plan.add(maps::panic_class(plan)?)?;
        plan.key_hasher = Some(c);
        let list = plan.list_repr(Repr::Tagged);
        plan.record(list)?;
    }
    Ok(result)
}
pub(super) struct Feature;
impl registry::Feature for Feature {
    fn reach(
        &self,
        db: &dyn Db,
        plan: &mut Plan<'_>,
        term: Option<&Terminator<'_>>,
    ) -> Option<Gen<registry::Reach>> {
        let term = term?;
        if let Terminator::SysOp {
            callee: Operand::Constant(Constant::Function(f)),
            ..
        } = term
        {
            if plan
                .index
                .function(db, *f)
                .is_some_and(|(id, _)| plan.index.display(id) == "baml.sys.collect_garbage")
            {
                return Some(Ok(registry::Reach::default()));
            }
        }
        if virtual_method(plan, term).is_some() {
            return Some(Ok(registry::Reach::default()));
        }
        let Terminator::Call {
            callee: Operand::Constant(Constant::Function(f)),
            ..
        } = term
        else {
            return None;
        };
        let (callee, _) = plan.index.function(db, *f)?;
        let name = plan.index.display(callee);
        matches!(
            name,
            "baml.hash.DefaultHasher.new" | "baml._hash_structural_default"
        )
        .then(|| {
            let ty = hasher_type(plan);
            let Repr::Ptr(c) = plan.repr(&ty)? else {
                unreachable!()
            };
            plan.add(c)?;
            plan.add(maps::panic_class(plan)?)?;
            plan.key_hasher = Some(c);
            let list = plan.list_repr(Repr::Tagged);
            plan.record(list)?;
            Ok(registry::Reach::default())
        })
    }
    fn call(
        &self,
        e: &Emitter<'_, '_>,
        name: &str,
        _types: &[Repr],
        values: &[(String, Repr)],
    ) -> Option<Gen<(String, Repr)>> {
        if name == "baml.hash.DefaultHasher.new" {
            return Some(Ok((
                e.fallible(&format!(
                    "rt::maps::callbacks::new_hasher({})",
                    e.descriptors[&e.plan.key_hasher.unwrap()]
                )),
                Repr::Ptr(e.plan.key_hasher.unwrap()),
            )));
        }
        if name == "baml._hash_structural_default" {
            return Some((|| {
                let value = e.coerce(values[0].clone(), Repr::Tagged)?;
                let state = e.coerce(values[1].clone(), Repr::Ptr(e.plan.key_hasher.unwrap()))?;
                Ok((e.fallible(&format!("rt::maps::callbacks::hash_into(&mut rt::strings::Cx::current(),{value},{state},&{},false).map(|_|0usize)",e.key_hooks()?)),Repr::Null))
            })());
        }
        None
    }
    fn terminator(&self, e: &mut Emitter<'_, '_>, term: &Terminator<'_>) -> Option<Gen<String>> {
        if let Terminator::SysOp {
            callee: Operand::Constant(Constant::Function(f)),
            destination,
            ..
        } = term
        {
            if e.plan
                .index
                .function(e.db, *f)
                .is_some_and(|(id, _)| e.plan.index.display(id) == "baml.sys.collect_garbage")
            {
                return Some(e.assign(
                    destination,
                    ("{rt::maps::callbacks::collect();0usize}".into(), Repr::Null),
                ));
            }
        }
        let (method, _) = virtual_method(e.plan, term)?;
        Some((|| {
            let Terminator::VirtualCall {
                args,
                ntypeargs,
                self_arg,
                destination,
                ..
            } = term
            else {
                unreachable!()
            };
            let values = args[*ntypeargs..]
                .iter()
                .map(|a| e.operand(a))
                .collect::<Gen<Vec<_>>>()?;
            let (expression, repr) = match method {
                "hash" => {
                    let value = e.coerce(values[*self_arg].clone(), Repr::Tagged)?;
                    let state = e.coerce(
                        values[1 - *self_arg].clone(),
                        Repr::Ptr(e.plan.key_hasher.ok_or_else(|| no("hash state"))?),
                    )?;
                    (e.fallible(&format!("rt::maps::callbacks::hash_into(&mut rt::strings::Cx::current(),{value},{state},&{},true).map(|_|0usize)",e.key_hooks()?)),Repr::Null)
                }
                "write" => {
                    let state = e.coerce(
                        values[*self_arg].clone(),
                        Repr::Ptr(e.plan.key_hasher.ok_or_else(|| no("hash state"))?),
                    )?;
                    let bytes = e.coerce(values[1 - *self_arg].clone(), Repr::Bytes)?;
                    (
                        format!(
                            "{{rt::maps::callbacks::write({state},&rt::bytes::data({bytes}));0usize}}"
                        ),
                        Repr::Null,
                    )
                }
                "finish" => {
                    let state = e.coerce(
                        values[*self_arg].clone(),
                        Repr::Ptr(e.plan.key_hasher.ok_or_else(|| no("hash state"))?),
                    )?;
                    (
                        format!("((rt::maps::callbacks::finish({state}) as i64)>>1)"),
                        Repr::Int,
                    )
                }
                _ => unreachable!(),
            };
            e.assign(destination, (expression, repr))
        })())
    }
}
impl Emitter<'_, '_> {
    pub(super) fn key_hooks(&self) -> Gen<String> {
        let c = self
            .plan
            .key_hasher
            .ok_or_else(|| no("managed hash state"))?;
        let snapshot = self.plan.list_repr(Repr::Tagged);
        let mut hash = String::from("|value,state|{match value.tag{");
        let mut equals = String::from("|a,b|{match (a.tag,b.tag){");
        for (&repr, methods) in &self.plan.key_callbacks {
            let tag = match repr {
                Repr::Enum(e) => format!("CAPTURE_ENUM_{e}_TAG"),
                Repr::Naming => "CAPTURE_NAMING_TAG".into(),
                _ => self.tag(repr)?.to_string(),
            };
            if let Some(&callee) = methods.get("hash") {
                let MirFunctionKind::Bytecode(body) = &self.all[&callee].kind else {
                    return Err(no("custom hash body"));
                };
                let arg = self.coerce(
                    ("value".into(), Repr::Tagged),
                    self.plan.repr(&body.locals[1].ty)?,
                )?;
                let state = self.coerce(
                    ("state".into(), Repr::Ptr(c)),
                    self.plan.repr(&body.locals[2].ty)?,
                )?;
                write!(hash,"{tag}=>{{f{callee}::<OBSERVE>(rt::calls::Context::current(),{},{arg},{state})?;Ok(true)}},",self.telemetry_site(callee)).unwrap();
            }
            if let Some(&callee) = methods.get("eq") {
                let MirFunctionKind::Bytecode(body) = &self.all[&callee].kind else {
                    return Err(no("custom equality body"));
                };
                let a = self.coerce(
                    ("a".into(), Repr::Tagged),
                    self.plan.repr(&body.locals[1].ty)?,
                )?;
                let b = self.coerce(
                    ("b".into(), Repr::Tagged),
                    self.plan.repr(&body.locals[2].ty)?,
                )?;
                write!(equals,"({tag},{tag})=>f{callee}::<OBSERVE>(rt::calls::Context::current(),{},{a},{b}).map(Some),",self.telemetry_site(callee)).unwrap();
            }
        }
        hash.push_str("_=>Ok(false)}}");
        equals.push_str("_=>Ok(None)}}");
        Ok(format!(
            "rt::maps::callbacks::Hooks{{shapes:CAPTURE_SHAPES,hash:{hash},equals:{equals},hasher_desc:{},snapshot_desc:{},panic:|cx,message|rt::maps::keys::panic(cx,{},message)}}",
            self.descriptors[&c],
            self.lists[&snapshot],
            self.descriptors[&maps::panic_class(self.plan)?]
        ))
    }
}
