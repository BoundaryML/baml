use super::*;

impl Plan<'_> {
    pub(super) fn implementors(
        &self,
        interface: usize,
    ) -> Gen<Vec<(usize, &bex_vm_types::types::ProgramImplRule)>> {
        let Object::Interface(def) =
            &self.program.objects[bex_vm_types::ObjectIndex::from_raw(interface)]
        else {
            return Err(no("non-interface dispatch"));
        };
        if def.structural_default.is_some()
            || !def.args.is_empty()
            || !def.assoc.is_empty()
            || !def.requires.is_empty()
        {
            return Err(no("generic or structural interface"));
        }
        let mut result = Vec::new();
        for package in &self.program.packages {
            if let Some(rules) = package
                .impl_rules
                .get(&bex_vm_types::ObjectIndex::from_raw(interface))
            {
                for rule in rules {
                    if !rule.generic_param_bounds.is_empty()
                        || !rule.interface_args.is_empty()
                        || !rule.interface_assoc.is_empty()
                    {
                        return Err(no("generic interface implementation"));
                    }
                    let bex_vm_types::TyTemplate::Class(head, args) = &rule.for_ty_pattern else {
                        return Err(no("non-class implementation"));
                    };
                    if !args.is_empty() {
                        return Err(no("generic implementor"));
                    }
                    result.push((self.class_tag(head.tag())?, rule));
                }
            }
        }
        if result.is_empty() {
            return Err(no("interface without concrete implementations"));
        }
        Ok(result)
    }
    pub(super) fn targets(&self, interface: usize, method: &str) -> Gen<Vec<(usize, usize)>> {
        self.implementors(interface)?
            .into_iter()
            .map(|(c, rule)| {
                let body = rule
                    .methods
                    .get(method)
                    .ok_or_else(|| no("adopted interface default"))?;
                if !body.frame.is_empty() {
                    return Err(no("generic dispatch frame"));
                }
                Ok((c, body.fqn.raw()))
            })
            .collect()
    }
}

pub(super) struct Feature;
impl registry::Feature for Feature {
    fn reach(
        &self,
        db: &dyn Db,
        plan: &mut Plan<'_>,
        term: Option<&Terminator<'_>>,
    ) -> Option<Gen<registry::Reach>> {
        (matches!(term, Some(Terminator::VirtualCall { iface, ntypeargs: 0, has_trace: false, self_arg: 0, .. }) if iface.generics.is_empty())).then(|| admit_call(db, plan, term))
    }

    fn terminator(
        &self,
        emitter: &mut Emitter<'_, '_>,
        term: &Terminator<'_>,
    ) -> Option<Gen<String>> {
        (matches!(term, Terminator::VirtualCall { iface, self_arg: 0, ntypeargs: 0, has_trace: false, .. } if iface.generics.is_empty())).then(|| emitter.dispatch_terminator(term))
    }
}
impl Emitter<'_, '_> {
    fn dispatch_terminator(&self, term: &Terminator<'_>) -> Gen<String> {
        let mut out = String::new();
        match term {
            Terminator::VirtualCall {
                iface,
                method,
                args,
                destination,
                argument_layout,
                self_arg: 0,
                ntypeargs: 0,
                has_trace: false,
                ..
            } if iface.generics.is_empty() => {
                let interface = self
                    .plan
                    .index
                    .interface_at(&iface.name)
                    .ok_or_else(|| no("virtual interface"))?;
                let targets = self.plan.targets(interface, method)?;
                let mut expr = String::from("{");
                let mut reps = Vec::new();
                for (i, arg) in args.iter().enumerate() {
                    let value = self.operand(arg)?;
                    let (v, repr) = if i == 0 {
                        (
                            self.coerce(value, Repr::Dyn(interface))?,
                            Repr::Dyn(interface),
                        )
                    } else {
                        value
                    };
                    write!(&mut expr, "let a{i}={v};").unwrap();
                    reps.push(repr);
                }
                expr.push_str("match a0.tag{");
                let mut returned = None;
                for (class, callee) in targets {
                    let mir = self
                        .all
                        .get(&callee)
                        .ok_or_else(|| no("dispatch body missing"))?;
                    let MirFunctionKind::Bytecode(body) = mir.kind.as_ref() else {
                        return Err(no("non-bytecode dispatch body"));
                    };
                    let mut values: Vec<_> = reps
                        .iter()
                        .enumerate()
                        .map(|(i, t)| {
                            if i == 0 {
                                ("a0.payload as usize".into(), Repr::Ptr(class))
                            } else {
                                (format!("a{i}"), *t)
                            }
                        })
                        .collect();
                    if let Some(layout) = argument_layout {
                        let mapping = layout
                            .map_to(&self.plan.index.info(callee).argument_layout())
                            .map_err(no)?;
                        values = mapping
                            .into_iter()
                            .map(|i| {
                                i.map(|i| values[i].clone())
                                    .ok_or_else(|| no("omitted dispatch argument"))
                            })
                            .collect::<Gen<_>>()?;
                    }
                    if values.len() != mir.arity {
                        return Err(no("dispatch arity"));
                    }
                    let arguments = values
                        .into_iter()
                        .enumerate()
                        .map(|(i, v)| self.coerce(v, self.plan.repr(&body.locals[i + 1].ty)?))
                        .collect::<Gen<Vec<_>>>()?
                        .join(",");
                    let ty = self.plan.repr(&body.locals[0].ty)?;
                    if returned.is_some_and(|r| r != ty) {
                        return Err(no("dispatch return conversion"));
                    }
                    returned = Some(ty);
                    write!(
                        &mut expr,
                        "{}=>f{callee}({},{arguments}),",
                        self.descriptors[&class] + 3,
                        self.telemetry_site(callee)
                    )
                    .unwrap();
                }
                expr.push_str(
                    "_=>Err(rt::Error::internal(\"invalid closed-world interface tag\".into()))}}",
                );
                out.push_str(&self.assign(
                    destination,
                    (
                        self.fallible(&expr),
                        returned.ok_or_else(|| no("empty dispatch"))?,
                    ),
                )?);
            }
            _ => unreachable!("feature terminator guard"),
        }
        Ok(out)
    }
}

fn admit_call(
    _db: &dyn Db,
    plan: &Plan<'_>,
    term: Option<&Terminator<'_>>,
) -> Gen<registry::Reach> {
    let mut reach = registry::Reach::default();
    match term {
        Some(Terminator::VirtualCall {
            iface,
            method,
            ntypeargs: 0,
            has_trace: false,
            self_arg: 0,
            ..
        }) if iface.generics.is_empty() => {
            let i = plan
                .index
                .interface_at(&iface.name)
                .ok_or_else(|| no("virtual interface"))?;
            for (_, target) in plan.targets(i, method)? {
                reach.targets.push(target);
            }
        }
        _ => unreachable!("reachability guard"),
    }
    Ok(reach)
}
