//! Closed-world interface witnesses. Resolution runs at pack time, including
//! bounds and defaults; receiver tags select already specialized typed bodies.
use super::*;
use baml_compiler2_mir::TyTemplateInterface;

pub(super) fn resolve(
    db: &dyn Db,
    plan: &Plan<'_>,
    concrete: &RuntimeTy,
    iface: &TyTemplateInterface,
    method: &str,
    method_args: &[RuntimeTy],
) -> Gen<Option<(usize, Vec<RuntimeTy>)>> {
    let Some(w) = witness(db, plan, concrete, iface)? else {
        return Ok(None);
    };
    let Witness {
        rule,
        bindings,
        interface: i,
        concrete,
    } = w;
    let (callee, frame) = if let Some(m) = rule.methods.get(method) {
        (
            m.fqn.raw(),
            m.frame
                .iter()
                .map(|t| t.substitute_symbolic(&bindings))
                .collect::<Vec<_>>(),
        )
    } else {
        let Object::Interface(def) = &plan.program.objects[bex_vm_types::ObjectIndex::from_raw(i)]
        else {
            unreachable!()
        };
        let default = def
            .methods
            .iter()
            .find(|m| m.name == method)
            .and_then(|m| m.default)
            .ok_or_else(|| no("interface method has no body"))?;
        let mut frame = vec![concrete.clone()];
        frame.extend(
            rule.interface_args
                .iter()
                .map(|t| t.substitute_symbolic(&bindings)),
        );
        (default.raw(), frame)
    };
    let mut frame = frame
        .iter()
        .map(|t| {
            t.try_map_heads(&mut |h| {
                plan.heads
                    .get(&h.tag())
                    .cloned()
                    .ok_or_else(|| no("unmapped implementation frame head"))
            })
        })
        .collect::<Gen<Vec<RuntimeTy>>>()?;
    frame.extend_from_slice(method_args);
    Ok(Some((callee, frame)))
}
struct Witness<'p> {
    rule: &'p bex_vm_types::types::ProgramImplRule,
    bindings: Vec<bex_vm_types::RuntimeTy>,
    interface: usize,
    concrete: bex_vm_types::RuntimeTy,
}
fn witness<'p>(
    db: &dyn Db,
    plan: &'p Plan<'_>,
    concrete: &RuntimeTy,
    iface: &TyTemplateInterface,
) -> Gen<Option<Witness<'p>>> {
    let requested = iface.to_template().substitute_symbolic(&[]);
    let RuntimeTy::Interface(name, args, assoc) = requested else {
        unreachable!()
    };
    let interface = baml_type::Interface {
        name,
        generics: args.iter().cloned().map(baml_type::Ty::from).collect(),
        associated_types: assoc
            .iter()
            .map(|(n, t)| (n.clone(), baml_type::Ty::from(t.clone())))
            .collect(),
    };
    // Reuse the checker's resolver for bound/coherence/alias semantics. The
    // linked rule supplies the exact body and its frame, rather than re-deriving
    // method identities from rendered names.
    let aliases = baml_compiler2_hir_ty::interfaces::package_resolved_aliases(db, plan.root);
    let Some(resolved) = baml_compiler2_hir_ty::interfaces::get_implements_block(
        db,
        plan.root,
        &baml_type::Ty::from(concrete.clone()),
        &interface,
        aliases,
    ) else {
        return Ok(None);
    };
    let data = baml_compiler2_hir_ty::interfaces::impl_data(db, resolved.impl_loc)
        .as_ref()
        .map_err(|_| no("checked implementation data missing"))?;
    let params = data
        .generic_params
        .iter()
        .map(|(p, _)| p.clone())
        .collect::<Vec<_>>();
    let runtime = baml_compiler2_mir::RuntimeLowering::of(db, plan.root);
    let anchor = |t: &baml_type::Ty| -> Gen<bex_vm_types::TyTemplate> {
        plan.index
            .anchor(&baml_compiler2_mir::tir2_to_template(t, &runtime, &params))
            .ok_or_else(|| no("implementation template head"))
    };
    let pattern = anchor(&data.for_ty_pattern)?;
    let iface_args = data
        .interface_args
        .iter()
        .map(anchor)
        .collect::<Gen<Vec<_>>>()?;
    let bindings = params
        .iter()
        .map(|p| {
            let ty = resolved
                .bindings
                .get(p)
                .ok_or_else(|| no("unbound checked implementation parameter"))?;
            let concrete = baml_compiler2_mir::RealizedTy::try_from(runtime.convert(ty))
                .map_err(|_| no("symbolic implementation binding"))?;
            Ok(plan
                .index
                .anchor(&baml_compiler2_mir::TyTemplate::from(concrete))
                .ok_or_else(|| no("implementation binding head"))?
                .substitute_symbolic(&[]))
        })
        .collect::<Gen<Vec<_>>>()?;
    let Some(anchored) = plan.index.anchor(&baml_compiler2_mir::TyTemplate::from(
        baml_compiler2_mir::RealizedTy::try_from(concrete.clone())
            .map_err(|_| no("non-concrete dispatch receiver"))?,
    )) else {
        return Err(no("unlinked dispatch receiver"));
    };
    let concrete = anchored.substitute_symbolic(&[]);
    let i = plan
        .index
        .interface_at(&iface.name)
        .ok_or_else(|| no("unlinked interface"))?;
    for package in &plan.program.packages {
        let Some(rules) = package
            .impl_rules
            .get(&bex_vm_types::ObjectIndex::from_raw(i))
        else {
            continue;
        };
        for rule in rules {
            if rule.for_ty_pattern != pattern || rule.interface_args != iface_args {
                continue;
            }
            return Ok(Some(Witness {
                rule,
                bindings,
                interface: i,
                concrete,
            }));
        }
    }
    Err(no("checked implementation missing from linked rules"))
}
pub(super) struct Feature;
impl registry::Feature for Feature {
    fn reach(
        &self,
        _: &dyn Db,
        _: &mut Plan<'_>,
        term: Option<&Terminator<'_>>,
    ) -> Option<Gen<registry::Reach>> {
        matches!(term, Some(Terminator::VirtualCall { .. }))
            .then(|| Err(no("interface without a reachable concrete receiver")))
    }
    fn terminator(&self, e: &mut Emitter<'_, '_>, term: &Terminator<'_>) -> Option<Gen<String>> {
        let Terminator::VirtualCall {
            args,
            ntypeargs,
            self_arg,
            destination,
            argument_layout,
            has_trace,
            ..
        } = term
        else {
            return None;
        };
        Some((|| {
            if *has_trace {
                return Err(no("interface invocation trace"));
            }
            let targets = e
                .plan
                .dispatches
                .get(&(e.object, e.block))
                .ok_or_else(|| no("missing closed-world dispatch"))?;
            let values = args[*ntypeargs..]
                .iter()
                .map(|a| e.operand(a))
                .collect::<Gen<Vec<_>>>()?;
            // Receiver-storage inference also sees through MIR's temporary
            // function-typed receiver copies. Concrete sites need one direct
            // target even when their interface is polymorphic elsewhere.
            let concrete = values[*self_arg].1;
            let narrowed = if matches!(concrete, Repr::Dyn(_) | Repr::Tagged) {
                Vec::new()
            } else {
                targets
                    .iter()
                    .copied()
                    .filter(|(r, _)| *r == concrete)
                    .collect::<Vec<_>>()
            };
            let targets = if narrowed.is_empty() {
                targets.as_slice()
            } else {
                narrowed.as_slice()
            };
            if let Some(result) = e.dispatch_vtable(term, targets) {
                return result;
            }
            let mut expression = String::from("{");
            for (i, (v, _)) in values.iter().enumerate() {
                write!(expression, "let a{i}={v};").unwrap();
            }
            let receiver = e.coerce((format!("a{self_arg}"), values[*self_arg].1), Repr::Tagged)?;
            write!(expression, "let receiver={receiver};match receiver.tag{{").unwrap();
            let returned = e.place(destination)?.1;
            let mut tags = BTreeSet::new();
            for &(repr, callee) in targets {
                let tag = match repr {
                    Repr::Naming => "CAPTURE_NAMING_TAG".into(),
                    Repr::Enum(i) => format!("CAPTURE_ENUM_{i}_TAG"),
                    _ => e.tag(repr)?.to_string(),
                };
                if !tags.insert(tag.clone()) {
                    return Err(no("overlapping dispatch storage tags"));
                }
                let MirFunctionKind::Bytecode(body) = &e.all[&callee].kind else {
                    return Err(no("interface builtin implementation"));
                };
                let mut call_values = values
                    .iter()
                    .enumerate()
                    .map(|(i, (_, r))| {
                        if i == *self_arg {
                            let raw = match repr {
                                Repr::Bool => "receiver.payload!=0".into(),
                                Repr::Float => "f64::from_bits(receiver.payload)".into(),
                                _ => format!("receiver.payload as {}", repr.rust()),
                            };
                            Ok((raw, repr))
                        } else {
                            Ok((format!("a{i}"), *r))
                        }
                    })
                    .collect::<Gen<Vec<_>>>()?;
                if let Some(layout) = argument_layout {
                    let mapping = layout
                        .map_to(&e.plan.index.info(callee).argument_layout())
                        .map_err(no)?;
                    call_values = mapping
                        .into_iter()
                        .map(|i| {
                            i.map(|i| call_values[i].clone())
                                .ok_or_else(|| no("omitted dispatch argument"))
                        })
                        .collect::<Gen<_>>()?;
                }
                if call_values.len() != e.all[&callee].arity {
                    return Err(no("interface call arity"));
                }
                let arguments = call_values
                    .into_iter()
                    .enumerate()
                    .map(|(i, v)| e.coerce(v, e.plan.repr(&body.locals[i + 1].ty)?))
                    .collect::<Gen<Vec<_>>>()?
                    .join(",");
                let value = e.fallible(&format!(
                    "f{callee}::<OBSERVE>(cx,{},{arguments})",
                    e.telemetry_site(callee)
                ));
                let result = e.coerce((value, e.plan.repr(&body.locals[0].ty)?), returned)?;
                write!(expression, "{tag}=>{{{result}}},").unwrap();
            }
            write!(expression,"_=>{} }}}}",e.fallible(&format!("Err::<{},_>(rt::Error::internal(\"invalid closed-world interface receiver\".into()))",returned.rust()))).unwrap();
            e.assign(destination, (expression, returned))
        })())
    }
}

impl Emitter<'_, '_> {
    // Tiny closed-world sets use guarded direct calls so LLVM can inline each
    // target. Wider class sites use an immutable descriptor-indexed vtable:
    // constant-time lookup, no runtime resolver, locks, or mutable cache.
    fn dispatch_vtable(
        &self,
        term: &Terminator<'_>,
        targets: &[(Repr, usize)],
    ) -> Option<Gen<String>> {
        if targets.len() <= 2 || !targets.iter().all(|(r, _)| matches!(r, Repr::Ptr(_))) {
            return None;
        }
        let Terminator::VirtualCall {
            args,
            ntypeargs,
            self_arg,
            destination,
            argument_layout,
            ..
        } = term
        else {
            return None;
        };
        let first = targets[0].1;
        let MirFunctionKind::Bytecode(body) = &self.all[&first].kind else {
            return None;
        };
        let signature = (1..=self.all[&first].arity)
            .map(|i| self.plan.repr(&body.locals[i].ty))
            .collect::<Gen<Vec<_>>>();
        let Ok(signature) = signature else {
            return None;
        };
        let Ok(returned) = self.plan.repr(&body.locals[0].ty) else {
            return None;
        };
        for &(_, callee) in targets {
            let MirFunctionKind::Bytecode(b) = &self.all[&callee].kind else {
                return None;
            };
            if self.all[&callee].arity != signature.len()
                || self.plan.repr(&b.locals[0].ty).ok() != Some(returned)
            {
                return None;
            }
            for (i, expected) in signature.iter().enumerate() {
                let Ok(actual) = self.plan.repr(&b.locals[i + 1].ty) else {
                    return None;
                };
                if actual.rust() != expected.rust() || i != *self_arg && actual != *expected {
                    return None;
                }
            }
            if self.plan.index.info(callee).argument_layout()
                != self.plan.index.info(first).argument_layout()
            {
                return None;
            }
        }
        Some((|| {
            let values = args[*ntypeargs..]
                .iter()
                .map(|a| self.operand(a))
                .collect::<Gen<Vec<_>>>()?;
            let mut code = String::from("{");
            for (i, (v, _)) in values.iter().enumerate() {
                write!(code, "let a{i}={v};").unwrap();
            }
            let receiver =
                self.coerce((format!("a{self_arg}"), values[*self_arg].1), Repr::Tagged)?;
            write!(code, "let receiver={receiver};").unwrap();
            let fn_ty = format!(
                "unsafe fn(rt::calls::Context,usize,{})->R<{}>",
                signature
                    .iter()
                    .map(|r| r.rust())
                    .collect::<Vec<_>>()
                    .join(","),
                returned.rust()
            );
            let mut table = BTreeMap::new();
            for &(repr, callee) in targets {
                table.insert(self.tag(repr)?, (callee, self.telemetry_site(callee)));
            }
            for (table_name, observe) in [("VTABLE_ON", "true"), ("VTABLE_OFF", "false")] {
                write!(code, "static {table_name}:&[Option<({fn_ty},usize)>]=&[").unwrap();
                for tag in 0..=*table.keys().last().unwrap() {
                    if let Some((callee, site)) = table.get(&tag) {
                        write!(code, "Some((f{callee}::<{observe}> as {fn_ty},{site})),").unwrap();
                    } else {
                        code.push_str("None,");
                    }
                }
                code.push_str("];");
            }
            code.push_str("let table=if OBSERVE{VTABLE_ON}else{VTABLE_OFF};let (callee,site)=");
            code.push_str(&self.fallible("table.get(receiver.tag as usize).copied().flatten().ok_or_else(||rt::Error::internal(\"invalid closed-world interface receiver\".into()))"));
            code.push(';');
            let mut values = values
                .iter()
                .enumerate()
                .map(|(i, (_, r))| {
                    if i == *self_arg {
                        ("receiver.payload as usize".into(), signature[i])
                    } else {
                        (format!("a{i}"), *r)
                    }
                })
                .collect::<Vec<_>>();
            if let Some(layout) = argument_layout {
                let mapping = layout
                    .map_to(&self.plan.index.info(first).argument_layout())
                    .map_err(no)?;
                values = mapping
                    .into_iter()
                    .map(|i| {
                        i.map(|i| values[i].clone())
                            .ok_or_else(|| no("omitted vtable argument"))
                    })
                    .collect::<Gen<_>>()?;
            }
            let arguments = values
                .into_iter()
                .zip(&signature)
                .map(|(v, r)| self.coerce(v, *r))
                .collect::<Gen<Vec<_>>>()?
                .join(",");
            code.push_str(&self.fallible(&format!("callee(cx,site,{arguments})")));
            code.push('}');
            self.assign(destination, (code, returned))
        })())
    }
}

pub(super) fn field_targets(
    db: &dyn Db,
    plan: &Plan<'_>,
    iface: &TyTemplateInterface,
    index: u32,
) -> Gen<Vec<(usize, usize)>> {
    let mut targets = Vec::new();
    for ty in &plan.concrete_types {
        let Some(w) = witness(db, plan, ty, iface)? else {
            continue;
        };
        let Repr::Ptr(c) = plan.repr(ty)? else {
            continue;
        };
        let field = *w
            .rule
            .field_links
            .get(index as usize)
            .ok_or_else(|| no("missing interface field link"))? as usize;
        if !targets.contains(&(c, field)) {
            targets.push((c, field));
        }
    }
    Ok(targets)
}

pub(super) fn implements(
    db: &dyn Db,
    plan: &Plan<'_>,
    concrete: &RuntimeTy,
    iface: &TyTemplateInterface,
) -> Gen<bool> {
    Ok(witness(db, plan, concrete, iface)?.is_some())
}
