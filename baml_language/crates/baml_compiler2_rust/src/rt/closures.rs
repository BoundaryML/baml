use super::{
    Gen, MirFunctionBody, Object, Operand, Place, Plan, Repr, RuntimeTy, Rvalue, StatementKind,
    Terminator, no, registry,
};

impl Plan<'_> {
    // Infer copy-assigned receiver storage; all definitions must agree.
    // Real callables retain a managed environment at every dynamic boundary.
    pub(super) fn linked_function_frame(&self, object: usize) -> Gen<Vec<bex_vm_types::RuntimeTy>> {
        self.function_frames
            .get(&object)
            .map(|frame| {
                frame
                    .iter()
                    .map(|(_, ty)| {
                        let realized = baml_compiler2_mir::RealizedTy::try_from(ty.clone())
                            .map_err(|_| no("generic builtin frame"))?;
                        self.index
                            .anchor(&baml_compiler2_mir::TyTemplate::from(realized))
                            .map(|t| t.substitute_symbolic(&[]))
                            .ok_or_else(|| no("generic builtin type"))
                    })
                    .collect()
            })
            .unwrap_or_else(|| Ok(Vec::new()))
    }
    pub(super) fn closure_type(&self, ty: &RuntimeTy) -> Gen<(u64, baml_type::RuntimeTy<usize>)> {
        let ty = ty.try_map_heads(&mut |head| {
            self.index
                .class_at(head)
                .map(|(id, _)| id)
                .ok_or_else(|| no("closure signature declaration"))
        })?;
        Ok((signature_key(&ty), ty))
    }
    pub(super) fn closure_field_type(
        &self,
        ty: &bex_vm_types::RuntimeTy,
    ) -> Gen<(u64, baml_type::RuntimeTy<usize>)> {
        let ty = ty.try_map_heads(&mut |head| self.class_tag(head.tag()))?;
        Ok((signature_key(&ty), ty))
    }
    pub(super) fn local_repr(
        &self,
        body: &MirFunctionBody<'_>,
        i: usize,
        depth: usize,
    ) -> Gen<Repr> {
        if body.blocks.iter().any(|b| matches!(&b.terminator, Some(Terminator::Spawn {future:Place::Local(l),..} | Terminator::Await {future:Place::Local(l),..}) if l.0==i)) { return Ok(Repr::Future); }
        if i > 0
            && self
                .lambda_bodies
                .get(&(body as *const _ as usize))
                .is_some_and(|id| self.index.info(*id).param_has_default.get(i - 1) == Some(&true))
        {
            return Ok(Repr::Tagged);
        }
        let definitions: Vec<_> = body
            .blocks
            .iter()
            .enumerate()
            .flat_map(|(block, b)| {
                b.statements
                    .iter()
                    .enumerate()
                    .map(move |(at, s)| (block, at, s))
            })
            .filter_map(|(block, at, s)| match &s.kind {
                StatementKind::Assign {
                    destination: Place::Local(l),
                    value,
                } if l.0 == i => Some((block, at, value)),
                _ => None,
            })
            .collect();
        if definitions
            .iter()
            .any(|(_, _, v)| matches!(v, Rvalue::LoadType(_)))
        {
            if i == 0
                || body.locals[i].name.is_some()
                || body.blocks.iter().any(|b| {
                    b.terminator
                        .as_ref()
                        .and_then(crate::analysis::continuation)
                        .is_some_and(|(p, _)| matches!(p,Place::Local(l) if l.0 == i))
                })
            {
                return Err(no("runtime type value"));
            }

            let mut code = None;
            for (_, _, value) in definitions {
                let Rvalue::LoadType(ty) = value else {
                    return Err(no("dynamic type storage"));
                };
                let next = self.type_code(ty)?;
                if code.is_some_and(|c| c != next) {
                    return Err(no("dynamic type argument"));
                }
                code = Some(next);
            }
            return Ok(Repr::Type(code.unwrap()));
        }
        if !matches!(body.locals[i].ty, RuntimeTy::Function { .. }) {
            return self.repr(&body.locals[i].ty);
        }
        // Method lookup temporarily stores its receiver in a function-typed
        // temporary. Keep that storage optimization, but real callables use a
        // managed environment irrespective of signature.
        if depth < body.locals.len() && i != 0 && body.locals[i].name.is_none() {
            let mut inferred = None;
            for (block, at, value) in definitions {
                if let Rvalue::Use(Operand::Copy(place) | Operand::Move(place)) = value {
                    if let Ok(repr) = self.list_field_repr(body, place, block, at, depth + 1) {
                        if inferred.is_some_and(|previous| previous != repr) {
                            return Ok(Repr::Closure);
                        }
                        inferred = Some(repr);
                    } else {
                        return Ok(Repr::Closure);
                    }
                } else {
                    return Ok(Repr::Closure);
                }
            }
            if let Some(repr) = inferred {
                return Ok(repr);
            }
        }
        Ok(Repr::Closure)
    }

    pub(super) fn storage_repr(
        &self,
        body: &MirFunctionBody<'_>,
        place: &Place,
        depth: usize,
    ) -> Gen<Repr> {
        match place {
            Place::Local(local) => self.local_repr(body, local.0, depth),
            Place::Deref(baml_compiler2_mir::CellId::Local(local)) => {
                self.local_repr(body, local.0, depth)
            }
            Place::Deref(baml_compiler2_mir::CellId::Capture(i)) => {
                let object = self
                    .lambda_bodies
                    .get(&(body as *const _ as usize))
                    .ok_or_else(|| no("capture owner"))?;
                self.captures
                    .get(object)
                    .and_then(|c| c.get(*i))
                    .copied()
                    .ok_or_else(|| no("capture storage"))
            }
            Place::Field { base, field } => {
                let Repr::Ptr(class) = self.storage_repr(body, base, depth)? else {
                    return Err(no("receiver field on non-class"));
                };
                self.field_repr(
                    self.class_field_types(class)
                        .get(*field)
                        .ok_or_else(|| no("receiver field"))?,
                )
            }
            Place::Index {
                base,
                kind: baml_compiler2_mir::IndexKind::Array,
                ..
            } => self.element(self.storage_repr(body, base, depth)?),
            _ => Err(no("receiver storage place")),
        }
    }
}

pub(super) struct Feature;
impl registry::Feature for Feature {
    fn prepare<'db>(
        &self,
        db: &'db dyn super::Db,
        _root: super::SourceRoot,
        plan: &mut Plan<'_>,
        all: &mut super::BTreeMap<usize, super::TestMir<'db>>,
    ) -> Gen<Vec<usize>> {
        fn register<'db>(
            db: &'db dyn super::Db,
            plan: &mut Plan<'_>,
            all: &mut super::BTreeMap<usize, super::TestMir<'db>>,
            mir: &super::MirFunction<'db>,
        ) -> Gen<()> {
            for lambda in &mir.lambdas {
                let object = plan
                    .index
                    .lambda(
                        db,
                        lambda
                            .identity
                            .as_ref()
                            .ok_or_else(|| no("lambda identity"))?,
                    )
                    .ok_or_else(|| no("unlinked lambda"))?;
                // Prepared tests already register borrowed lambda MIR. Keep it
                // shared instead of cloning every corpus lambda for each leaf.
                all.entry(object)
                    .or_insert_with(|| std::rc::Rc::new(lambda.clone()));
                if let super::MirFunctionKind::Bytecode(body) = &lambda.kind {
                    plan.lambda_bodies.insert(body as *const _ as usize, object);
                }
                register(db, plan, all, lambda)?;
            }
            Ok(())
        }
        for mir in all.values().cloned().collect::<Vec<_>>() {
            register(db, plan, all, &mir)?;
        }
        Ok(Vec::new())
    }
    fn call(
        &self,
        emitter: &super::Emitter<'_, '_>,
        name: &str,
        _types: &[Repr],
        values: &[(String, Repr)],
    ) -> Option<Gen<(String, Repr)>> {
        if name == "baml.ops.equals_equals"
            && values.len() == 2
            && values.iter().all(|v| v.1 == Repr::Closure)
        {
            return Some(Ok((
                format!("rt::closures::equal({},{})", values[0].0, values[1].0),
                Repr::Bool,
            )));
        }
        if !matches!(name, "baml.Array.push" | "baml.Array.length")
            || values
                .first()
                .is_none_or(|v| !matches!(v.1, Repr::ListClosure(_)))
        {
            return None;
        }
        Some((|| {
            let list = format!(
                "rt::lists::List::<rt::closures::Closure>::from_raw({})",
                values[0].0
            );
            if name == "baml.Array.length" && values.len() == 1 {
                return Ok((
                    format!("{list}.len(&rt::strings::Cx::current())"),
                    Repr::Int,
                ));
            }
            if name == "baml.Array.push" && values.len() == 2 {
                let value = emitter.coerce(values[1].clone(), Repr::Closure)?;
                return Ok((emitter.fallible(&format!("{list}.push(&mut rt::strings::Cx::current(),rt::closures::Closure::from_raw({value}))")),Repr::Int));
            }
            Err(no("closure list arguments"))
        })())
    }
    fn reach(
        &self,
        _db: &dyn super::Db,
        _plan: &mut Plan<'_>,
        term: Option<&Terminator<'_>>,
    ) -> Option<Gen<registry::Reach>> {
        matches!(
            term,
            Some(Terminator::Call {
                callee: Operand::Copy(_) | Operand::Move(_),
                ntypeargs: 0,
                has_trace: false,
                ..
            })
        )
        .then(|| Ok(registry::Reach::default()))
    }
    fn value(
        &self,
        emitter: &super::Emitter<'_, '_>,
        value: &Rvalue<'_>,
        _expected: Repr,
    ) -> Option<Gen<(String, Repr)>> {
        if let Rvalue::BinaryOp { op, left, right } = value {
            if matches!(op, super::BinOp::Eq | super::BinOp::Ne) {
                if let (Ok((l, Repr::Closure)), Ok((r, Repr::Closure))) =
                    (emitter.operand(left), emitter.operand(right))
                {
                    let negate = if *op == super::BinOp::Ne { "!" } else { "" };
                    return Some(Ok((
                        format!("{negate}rt::closures::equal({l},{r})"),
                        Repr::Bool,
                    )));
                }
            }
            if matches!(op, super::BinOp::Eq | super::BinOp::Ne)
                && (matches!(left, Operand::Constant(super::Constant::OmittedArg))
                    || matches!(right, Operand::Constant(super::Constant::OmittedArg)))
            {
                return Some((|| {
                    let other = if matches!(right, Operand::Constant(super::Constant::OmittedArg)) {
                        left
                    } else {
                        right
                    };
                    let raw = match other {
                        Operand::Copy(p) | Operand::Move(p) => emitter.place(p)?,
                        _ => emitter.operand(other)?,
                    };
                    let expression = if raw.1 == Repr::Tagged {
                        format!("({}).tag==u64::MAX", raw.0)
                    } else {
                        "false".into()
                    };
                    Ok((
                        if *op == super::BinOp::Ne {
                            format!("!({expression})")
                        } else {
                            expression
                        },
                        Repr::Bool,
                    ))
                })());
            }
        }
        if matches!(
            value,
            Rvalue::MakeVirtualBoundMethod { .. } | Rvalue::MakeVirtualFunction { .. }
        ) {
            return Some((|| {
                let targets = emitter
                    .plan
                    .virtual_values
                    .get(&(emitter.object, emitter.block, emitter.statement))
                    .ok_or_else(|| no("interface callable has no concrete target"))?;
                if let Rvalue::MakeVirtualBoundMethod { receiver, .. } = value {
                    let receiver = emitter.coerce(emitter.operand(receiver)?, Repr::Tagged)?;
                    let mut cases = String::new();
                    for &(repr, object) in targets {
                        let bind = if emitter
                            .plan
                            .function_frames
                            .get(&object)
                            .is_some_and(|f| !f.is_empty())
                        {
                            "bind_generic"
                        } else {
                            "bind"
                        };
                        cases.push_str(&format!(
                            "{}=>rt::closures::{bind}({},{},{object},closure{object},receiver),",
                            emitter.tag(repr)?,
                            emitter.plan.closure_descriptors().unwrap().0,
                            emitter.plan.closure_descriptors().unwrap().1
                        ));
                    }
                    Ok((emitter.fallible(&format!("{{let receiver={receiver};match receiver.tag{{{cases}_=>Err(rt::Error::internal(\"invalid bound interface receiver\".into()))}}}}")),Repr::Closure))
                } else {
                    let [(_, object)] = targets.as_slice() else {
                        return Err(no("ambiguous interface function value"));
                    };
                    let function = if emitter
                        .plan
                        .function_frames
                        .get(object)
                        .is_some_and(|f| !f.is_empty())
                    {
                        "generic_function"
                    } else {
                        "function"
                    };
                    let name = if function == "function" {
                        format!(",{:?}", emitter.plan.index.info(*object).name().as_str())
                    } else {
                        String::new()
                    };
                    Ok((
                        emitter.fallible(&format!(
                            "rt::closures::{function}({}, {object},closure{object}{name})",
                            emitter.plan.closure_descriptors().unwrap().1
                        )),
                        Repr::Closure,
                    ))
                }
            })());
        }
        if let Rvalue::MakeBoundMethod {
            func,
            receiver,
            frame: _,
        } = value
        {
            return Some((|| {
                let object = emitter
                    .plan
                    .generic_values
                    .get(&(emitter.object, emitter.block, emitter.statement))
                    .copied()
                    .or_else(|| {
                        emitter
                            .plan
                            .index
                            .function(emitter.db, *func)
                            .map(|(id, _)| id)
                    })
                    .ok_or_else(|| no("bound method target"))?;
                let receiver = emitter.coerce(emitter.operand(receiver)?, Repr::Tagged)?;
                let bind = if emitter
                    .plan
                    .function_frames
                    .get(&object)
                    .is_some_and(|f| !f.is_empty())
                {
                    "bind_generic"
                } else {
                    "bind"
                };
                Ok((
                    emitter.fallible(&format!(
                        "rt::closures::{bind}({},{},{object},closure{object},{receiver})",
                        emitter.plan.closure_descriptors().unwrap().0,
                        emitter.plan.closure_descriptors().unwrap().1
                    )),
                    Repr::Closure,
                ))
            })());
        }
        if let Rvalue::MakeGenericFunction { .. } = value {
            return Some((|| {
                let object = emitter.plan.generic_values
                    [&(emitter.object, emitter.block, emitter.statement)];
                Ok((
                    emitter.fallible(&format!(
                        "rt::closures::generic_function({}, {object},closure{object})",
                        emitter.plan.closure_descriptors().unwrap().1
                    )),
                    Repr::Closure,
                ))
            })());
        }
        let Rvalue::MakeClosure {
            lambda_idx,
            captures,
            type_arg_templates,
        } = value
        else {
            return None;
        };
        Some((|| {
            let _ = type_arg_templates;
            let object = emitter.plan.lambda_targets[&(emitter.object, *lambda_idx)];
            let captures = captures
                .iter()
                .map(|cell| emitter.operand(cell).map(|v| v.0))
                .collect::<Gen<Vec<_>>>()?
                .join(",");
            let ordinary = emitter.fallible(&format!(
                "rt::closures::new({}, {object},closure{object},&[{captures}])",
                emitter.plan.closure_descriptors().unwrap().1
            ));
            Ok((ordinary, Repr::Closure))
        })())
    }
    fn statement(
        &self,
        emitter: &mut super::Emitter<'_, '_>,
        statement: &StatementKind<'_>,
    ) -> Option<Gen<String>> {
        let StatementKind::FreshCell { local, carry_value } = statement else {
            return None;
        };
        Some((|| {
            let i = local.0;
            let value = if *carry_value {
                format!("rt::closures::load(l{i})")
            } else {
                "rt::Dyn{tag:2,payload:0}".into()
            };
            Ok(format!(
                "l{i}={};",
                emitter.fallible(&format!(
                    "rt::closures::cell({},{value})",
                    emitter.plan.closure_descriptors().unwrap().0
                ))
            ))
        })())
    }
    fn terminator(
        &self,
        emitter: &mut super::Emitter<'_, '_>,
        term: &Terminator<'_>,
    ) -> Option<Gen<String>> {
        let Terminator::Call {
            callee: callee @ (Operand::Copy(_) | Operand::Move(_)),
            args,
            ntypeargs: 0,
            destination,
            argument_layout,
            ..
        } = term
        else {
            return None;
        };
        Some((|| {
            let callable = emitter.coerce(emitter.operand(callee)?, Repr::Closure)?;
            let arguments = args
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    Ok(format!(
                        "let a{i}={};",
                        emitter.coerce(emitter.operand(v)?, Repr::Tagged)?
                    ))
                })
                .collect::<Gen<Vec<_>>>()?
                .join("");
            let mut branches = String::new();
            for &object in &emitter.plan.callables {
                let site = emitter.telemetry_site(object);
                let positional = (0..args.len())
                    .map(|i| format!("a{i}"))
                    .collect::<Vec<_>>()
                    .join(",");
                let call = if let Some(layout) = argument_layout {
                    let mut target = emitter.plan.index.info(object).argument_layout();
                    let mut mapped = |bound: bool| {
                        if bound && !target.is_empty() {
                            target.0.remove(0);
                        }
                        match layout.map_to(&target) {
                            Ok(indices)=> {
                                let values=indices.into_iter().map(|i|i.map(|i|format!("a{i}")).unwrap_or_else(||"rt::Dyn{tag:u64::MAX,payload:0}".into())).collect::<Vec<_>>().join(",");
                                format!("rt::closures::call(env,{site},&[{values}])")
                            },
                            Err(_)=>"Err(rt::Error::internal(\"invalid callable argument layout\".into()))".into(),
                        }
                    };
                    let unbound = mapped(false);
                    let bound = mapped(true);
                    format!("if rt::closures::is_bound(env){{{bound}}}else{{{unbound}}}")
                } else {
                    format!("rt::closures::call(env,{site},&[{positional}])")
                };
                branches.push_str(&format!("{object}=>{call},"));
            }
            let object = emitter.fallible("rt::closures::object(env)");
            let call = format!(
                "{{let env={callable};{arguments}match {object}{{{branches}_=>unreachable!()}}}}"
            );
            emitter.assign(destination, (emitter.fallible(&call), Repr::Tagged))
        })())
    }
}

/// Discover callable values in a reached body, preserving each capture's type.
pub(super) fn discover<'db>(
    db: &'db dyn super::Db,
    plan: &mut Plan<'_>,
    object: usize,
    mir: &super::MirFunction<'db>,
) -> Gen<Vec<usize>> {
    let MirFunctionBody { blocks, .. } = match &mir.kind {
        super::MirFunctionKind::Bytecode(body) => body,
        _ => return Ok(Vec::new()),
    };
    let body = match &mir.kind {
        super::MirFunctionKind::Bytecode(body) => body,
        _ => unreachable!(),
    };
    let mut targets = plan
        .generic_values
        .iter()
        .filter_map(|(&(parent, _, _), &target)| (parent == object).then_some(target))
        .collect::<Vec<_>>();
    plan.callables.extend(targets.iter().copied());
    for (block, statement_index, statement) in blocks.iter().flat_map(|b| {
        b.statements
            .iter()
            .enumerate()
            .map(move |(i, s)| (b.id.0, i, s))
    }) {
        if let StatementKind::Assign {
            value:
                Rvalue::MakeClosure {
                    lambda_idx,
                    captures,
                    ..
                },
            ..
        } = &statement.kind
        {
            let target = plan.lambda_targets[&(object, *lambda_idx)];
            let types = captures
                .iter()
                .map(|o| match o {
                    Operand::Copy(Place::Local(l)) | Operand::Move(Place::Local(l)) => {
                        plan.local_repr(body, l.0, 0)
                    }
                    Operand::Copy(Place::Capture(i)) | Operand::Move(Place::Capture(i)) => plan
                        .captures
                        .get(&object)
                        .and_then(|c| c.get(*i))
                        .copied()
                        .ok_or_else(|| no("forwarded capture")),
                    _ => Err(no("closure capture must be a cell")),
                })
                .collect::<Gen<Vec<_>>>()?;
            for &ty in &types {
                plan.record(ty)?;
            }
            plan.captures.insert(target, types);
            plan.callables.insert(target);
            targets.push(target);
        }
        if let StatementKind::Assign {
            value: Rvalue::MakeBoundMethod { func, .. },
            ..
        } = &statement.kind
        {
            let target = plan
                .generic_values
                .get(&(object, block, statement_index))
                .copied()
                .or_else(|| plan.index.function(db, *func).map(|(id, _)| id))
                .ok_or_else(|| no("bound method target"))?;
            plan.callables.insert(target);
            targets.push(target);
        }
        if let Some(cases) = plan
            .virtual_values
            .get(&(object, block, statement_index))
            .cloned()
        {
            for (repr, target) in cases {
                plan.record(repr)?;
                plan.callables.insert(target);
                targets.push(target);
            }
        }
        crate::operands::walk_statement_operands(&statement.kind, &mut |operand| {
            if let Operand::Constant(super::Constant::GenericFunction { func, type_args }) = operand
            {
                if let Some((base, _)) = plan.index.function(db, *func) {
                    let frame = type_args
                        .iter()
                        .map(|t| super::RuntimeTy::from(t.clone()))
                        .collect::<Vec<_>>();
                    if let Some((_, _, target)) = plan
                        .generic_functions
                        .iter()
                        .find(|(c, t, _)| *c == base && *t == frame)
                    {
                        plan.callables.insert(*target);
                        targets.push(*target);
                    }
                }
            }
            if let Operand::Constant(super::Constant::Function(f)) = operand {
                if let Some((target, _)) = plan.index.function(db, *f) {
                    plan.callables.insert(target);
                    targets.push(target);
                }
            }
        });
    }
    for term in blocks.iter().filter_map(|b| b.terminator.as_ref()) {
        let args = match term {
            Terminator::Call { args, .. } | Terminator::VirtualCall { args, .. } => args.as_slice(),
            _ => &[],
        };
        for operand in args {
            if let Operand::Constant(super::Constant::GenericFunction { func, type_args }) = operand
            {
                let (base, _) = plan
                    .index
                    .function(db, *func)
                    .ok_or_else(|| no("generic callback"))?;
                let frame = type_args
                    .iter()
                    .map(|t| super::RuntimeTy::from(t.clone()))
                    .collect::<Vec<_>>();
                let target = plan
                    .generic_functions
                    .iter()
                    .find(|(c, t, _)| *c == base && *t == frame)
                    .map(|(_, _, id)| *id)
                    .ok_or_else(|| no("generic callback instance"))?;
                plan.callables.insert(target);
                targets.push(target);
            }
            if let Operand::Constant(super::Constant::Function(f)) = operand {
                let (target, _) = plan
                    .index
                    .function(db, *f)
                    .ok_or_else(|| no("function argument target"))?;
                plan.callables.insert(target);
                targets.push(target);
            }
        }
    }
    Ok(targets)
}

impl super::Emitter<'_, '_> {
    pub(super) fn closure_cell(&self, cell: &baml_compiler2_mir::CellId) -> Gen<(String, Repr)> {
        Ok(match cell {
            baml_compiler2_mir::CellId::Local(l) => (
                format!("l{}", l.0),
                self.plan.local_repr(self.body, l.0, 0)?,
            ),
            baml_compiler2_mir::CellId::Capture(i) => (
                format!("rt::closures::capture(env,{i})"),
                *self
                    .plan
                    .captures
                    .get(&self.object)
                    .and_then(|c| c.get(*i))
                    .ok_or_else(|| no("capture type"))?,
            ),
        })
    }
    pub(super) fn closure_argument(&self, value: String, repr: Repr) -> Gen<String> {
        if matches!(repr, Repr::Tagged | Repr::Dyn(_)) {
            return Ok(value);
        }
        let tag = self.tag(repr)?;
        let raw = format!("rt::untag({value},{tag},{})?", repr.pointer());
        Ok(repr.decode_payload(&raw))
    }
    pub(super) fn closure_adapter(&self) -> Gen<String> {
        if !self.plan.callables.contains(&self.object) {
            return Ok(String::new());
        }
        let object = self.object;
        let arguments = (1..=self.arity)
            .map(|i| {
                let value = if self.plan.captures.contains_key(&self.object) {
                    format!("args.get({}).copied().unwrap_or(rt::Dyn{{tag:u64::MAX,payload:0}})",i-1)
                } else if i == 1 {
                    "if rt::closures::is_bound(env){rt::closures::load(rt::closures::capture(env,0))}else{args.get(0).copied().unwrap_or(rt::Dyn{tag:u64::MAX,payload:0})}".to_owned()
                } else {
                    format!("args.get(if rt::closures::is_bound(env){{{}}}else{{{}}}).copied().unwrap_or(rt::Dyn{{tag:u64::MAX,payload:0}})",i-2,i-1)
                };
                self.closure_argument(value, self.argument_repr(i)?)
            })
            .collect::<Gen<Vec<_>>>()?
            .join(",");
        let env = if self.plan.captures.contains_key(&object) {
            "env,"
        } else {
            ""
        };
        let value = self.coerce(
            (format!("value"), self.plan.local_repr(self.body, 0, 0)?),
            Repr::Tagged,
        )?;
        Ok(format!(
            "unsafe fn closure{object}(env:usize,site:usize,args:&[rt::Dyn])->R<rt::Dyn>{{unsafe{{let cx=rt::calls::Context::current();let value=if rt::telemetry::active(){{f{object}::<true>(cx,site,{env}{arguments})}}else{{f{object}::<false>(cx,site,{env}{arguments})}}?;Ok({value})}}}}\n"
        ))
    }
}

pub(super) fn signature_key(ty: &baml_type::RuntimeTy<usize>) -> u64 {
    // Stable across MIR's declaration heads and linked field metadata.
    borsh::to_vec(ty)
        .expect("runtime type")
        .iter()
        .fold(0xcbf29ce484222325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        })
}

pub(super) fn capture_type(ty: &baml_type::RuntimeTy<usize>) -> Gen<String> {
    use baml_type::RuntimeTy as T;
    let body = match ty {
        T::Future(value, error) => format!(
            "Future(&{},&{})",
            capture_type(value)?,
            capture_type(error)?
        ),
        T::Float => "Float".into(),
        T::Enum(id) => format!("Enum(CAPTURE_D{id})"),
        T::Bigint => "Bigint".into(),
        T::Int => "Int".into(),
        T::Bool => "Bool".into(),
        T::String => "String".into(),
        T::Null => "Null".into(),
        T::Never => "Never".into(),
        T::Unknown => "Unknown".into(),
        T::Class(id, args) if args.is_empty() => format!("Class(CAPTURE_D{id})"),
        T::List(t) => format!("List(&{})", capture_type(t)?),
        T::Union(types) => format!(
            "Union(&[{}])",
            types
                .iter()
                .map(capture_type)
                .collect::<Gen<Vec<_>>>()?
                .join(",")
        ),
        T::Function {
            params,
            ret,
            throws,
        } => {
            let params = params
                .iter()
                .map(|p| {
                    Ok(format!(
                        "rt::telemetry::capture::Parameter{{name:{:?},optional:{},ty:&{}}}",
                        p.name.as_ref().map(|n| n.as_str()),
                        p.mode == baml_type::FunctionParamMode::Optional,
                        capture_type(&p.ty)?
                    ))
                })
                .collect::<Gen<Vec<_>>>()?
                .join(",");
            format!(
                "Function{{params:&[{params}],ret:&{},throws:&{}}}",
                capture_type(ret)?,
                capture_type(throws)?
            )
        }
        _ => return Err(no("closure capture signature")),
    };
    Ok(format!("rt::telemetry::capture::Type::{body}"))
}

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
