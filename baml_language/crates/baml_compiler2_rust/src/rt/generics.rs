//! Closed-world specializations. Each concrete call gets ordinary typed MIR;
//! source coordinates continue to refer to the original declaration.
use super::*;
use baml_compiler2_hir_ty::callable::callable_generic_frame;
use baml_compiler2_mir::{TyTemplate, TypeTest};

pub(super) fn expand<'db>(
    db: &'db dyn Db,
    plan: &mut Plan<'_>,
    all: &mut BTreeMap<usize, MirFunction<'db>>,
    roots: Vec<usize>,
) -> Gen<()> {
    let mut queue = VecDeque::from(roots);
    let mut seen = BTreeSet::new();
    let mut instances: Vec<(usize, Vec<RuntimeTy>, usize)> = Vec::new();
    loop {
        let type_count = plan.concrete_types.len();
        while let Some(object) = queue.pop_front() {
            if !seen.insert(object) {
                continue;
            }
            let Some(mut mir) = all.get(&object).cloned() else {
                continue;
            };
            specialize_bound_values(plan, object, &mut mir)?;
            all.insert(object, mir.clone());
            let MirFunctionKind::Bytecode(body) = &mir.kind else {
                continue;
            };
            // Materialized lambdas belong to this concrete parent instance.
            // Keep the source identity for positions/telemetry, but never share
            // typed MIR or capture layouts between distinct generic frames.
            for (i, lambda) in mir.lambdas.iter().enumerate() {
                let key = (object, i);
                let target = if let Some(&target) = plan.lambda_targets.get(&key) {
                    target
                } else {
                    let origin = plan
                        .index
                        .lambda(db, &lambda.identity)
                        .ok_or_else(|| no("unlinked generic lambda"))?;
                    let target = if plan.function_frames.contains_key(&object) {
                        let target = plan.next_object;
                        plan.next_object += 1;
                        plan.index.rt_alias(target, origin);
                        plan.function_frames
                            .insert(target, plan.function_frames[&object].clone());
                        all.insert(target, lambda.clone());
                        target
                    } else {
                        origin
                    };
                    plan.lambda_targets.insert(key, target);
                    target
                };
                queue.push_back(target);
            }
            // Function constants may occur in returns, fields and builtin
            // callback arguments as well as ordinary calls.
            let mut function_values = Vec::new();
            for b in &body.blocks {
                let mut visit = |operand: &Operand<'db>| match operand {
                    Operand::Constant(Constant::Function(f)) => {
                        if let Some((id, _)) = plan.index.function(db, *f) {
                            function_values.push((id, Vec::new()));
                        }
                    }
                    Operand::Constant(Constant::GenericFunction { func, type_args }) => {
                        if let Some((id, _)) = plan.index.function(db, *func) {
                            function_values.push((
                                id,
                                type_args
                                    .iter()
                                    .map(|t| RuntimeTy::from(t.clone()))
                                    .collect(),
                            ));
                        }
                    }
                    _ => {}
                };
                for statement in &b.statements {
                    crate::operands::walk_statement_operands(&statement.kind, &mut visit);
                }
                if let Some(Terminator::Call { args, .. } | Terminator::VirtualCall { args, .. }) =
                    &b.terminator
                {
                    for arg in args {
                        visit(arg);
                    }
                }
            }
            for (callee, frame) in function_values {
                if frame.is_empty() {
                    queue.push_back(callee);
                } else {
                    queue.push_back(instance(db, plan, all, &mut instances, callee, frame)?);
                }
            }
            for b in &body.blocks {
                for (i, statement) in b.statements.iter().enumerate() {
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
                            .map(|c| {
                                operand_ty(plan, object, body, c)
                                    .ok_or_else(|| no("capture runtime type"))
                            })
                            .collect::<Gen<Vec<_>>>()?;
                        plan.capture_types.insert(target, types);
                    }
                    if let StatementKind::Assign {
                        value:
                            Rvalue::MakeBoundMethod {
                                func,
                                receiver,
                                frame,
                            },
                        ..
                    } = &statement.kind
                    {
                        let (callee, _) = plan
                            .index
                            .function(db, *func)
                            .ok_or_else(|| no("generic bound target"))?;
                        let types = match frame {
                            baml_compiler2_mir::BoundFrame::Explicit(types) => {
                                types.iter().map(|t| t.substitute_symbolic(&[])).collect()
                            }
                            baml_compiler2_mir::BoundFrame::Receiver => {
                                match operand_ty(plan, object, body, receiver) {
                                    Some(RuntimeTy::Class(_, types)) => types.to_vec(),
                                    Some(RuntimeTy::List(element)) => vec![*element],
                                    Some(RuntimeTy::Map { key, value }) => vec![*key, *value],
                                    _ => Vec::new(),
                                }
                            }
                        };
                        let target = instance(db, plan, all, &mut instances, callee, types)?;
                        plan.generic_values.insert((object, b.id.0, i), target);
                        queue.push_back(target);
                    }
                    if let StatementKind::Assign {
                        value:
                            Rvalue::MakeGenericFunction {
                                func,
                                type_arg_templates,
                            },
                        ..
                    } = &statement.kind
                    {
                        let (callee, _) = plan
                            .index
                            .function(db, *func)
                            .ok_or_else(|| no("generic function value"))?;
                        let frame = type_arg_templates
                            .iter()
                            .map(|t| t.substitute_symbolic(&[]))
                            .collect();
                        let target = instance(db, plan, all, &mut instances, callee, frame)?;
                        plan.generic_values.insert((object, b.id.0, i), target);
                        queue.push_back(target);
                    }
                }
            }
            let iters = crate::analysis::array_iterators(db, body, mir.arity);
            for (i, local) in body.locals.iter().enumerate() {
                if iters.contains(&i) {
                    continue;
                }
                plan.register_class_type(&local.ty)?;
                plan.candidate(&local.ty);
            }
            let mut constants = Vec::new();
            let mut constant = |operand: &Operand<'_>| {
                if let Operand::Constant(c) = operand {
                    let ty = match c {
                        Constant::Int(_) => Some(RuntimeTy::Int),
                        Constant::Bool(_) => Some(RuntimeTy::Bool),
                        Constant::Float(_) => Some(RuntimeTy::Float),
                        Constant::String(_) => Some(RuntimeTy::String),
                        Constant::Null => Some(RuntimeTy::Null),
                        Constant::EnumVariant { enum_ref, .. } => Some(RuntimeTy::Enum(
                            baml_compiler2_hir_ty::layout::enum_head(db, *enum_ref),
                        )),
                        _ => None,
                    };
                    if let Some(ty) = ty {
                        constants.push(ty);
                    }
                }
            };
            for b in &body.blocks {
                for s in &b.statements {
                    crate::operands::walk_statement_operands(&s.kind, &mut constant);
                }
                if let Some(t) = &b.terminator {
                    crate::operands::walk_terminator_operands(t, &mut constant);
                }
            }
            for t in constants {
                plan.candidate(&t);
            }
            for ty in plan.concrete_types.clone() {
                let RuntimeTy::Class(_, args) = &ty else {
                    continue;
                };
                let Repr::Ptr(class) = plan.repr(&ty)? else {
                    continue;
                };
                let info = plan.class(class);
                if let Some(cleanup) = info.cleanup {
                    let callee = info
                        .methods
                        .get_index(cleanup)
                        .expect("cleanup method")
                        .1
                        .function
                        .raw();
                    let target = instance(db, plan, all, &mut instances, callee, args.to_vec())?;
                    plan.cleanup_methods.insert(class, target);
                    queue.push_back(target);
                }
            }
            for b in &body.blocks {
                for (statement, s) in b.statements.iter().enumerate() {
                    if let StatementKind::Assign { value, .. } = &s.kind {
                        let virtual_value = match value {
                            Rvalue::MakeVirtualBoundMethod {
                                iface,
                                method,
                                receiver,
                                type_args,
                            } => Some((
                                iface,
                                method,
                                operand_ty(plan, object, body, receiver),
                                type_args
                                    .iter()
                                    .map(|t| t.substitute_symbolic(&[]))
                                    .collect::<Vec<_>>(),
                            )),
                            Rvalue::MakeVirtualFunction {
                                iface,
                                method,
                                self_ty,
                                type_args,
                            } => Some((
                                iface,
                                method,
                                Some(self_ty.substitute_symbolic(&[])),
                                type_args
                                    .iter()
                                    .map(|arg| type_operand(body, arg))
                                    .collect::<Gen<Vec<_>>>()?,
                            )),
                            _ => None,
                        };
                        if let Some((iface, method, receiver, method_args)) = virtual_value {
                            plan.remember_heads(&iface.to_template().substitute_symbolic(&[]));
                            let candidates = match receiver {
                                Some(RuntimeTy::Union(types)) => types.to_vec(),
                                Some(
                                    RuntimeTy::Interface(..)
                                    | RuntimeTy::Unknown
                                    | RuntimeTy::Function { .. },
                                )
                                | None => plan.concrete_types.clone(),
                                Some(concrete) => vec![concrete],
                            };
                            let mut targets = Vec::new();
                            for concrete in candidates {
                                if let Some((callee, frame)) = super::dispatch::resolve(
                                    db,
                                    plan,
                                    &concrete,
                                    iface,
                                    method,
                                    &method_args,
                                )? {
                                    let target =
                                        instance(db, plan, all, &mut instances, callee, frame)?;
                                    let repr = plan.repr(&concrete)?;
                                    if !targets.contains(&(repr, target)) {
                                        targets.push((repr, target));
                                    }
                                    queue.push_back(target);
                                }
                            }
                            if !targets.is_empty() {
                                plan.virtual_values
                                    .insert((object, b.id.0, statement), targets);
                            }
                        }

                        // A constructor can write straight into an interface or
                        // unknown slot; its concrete type need not occur in any
                        // local declaration (e.g. `return Square { ... }`).
                        let ty = match value {
                            Rvalue::Aggregate {
                                kind:
                                    AggregateKind::Class {
                                        class,
                                        type_arg_templates,
                                    },
                                ..
                            } => Some(RuntimeTy::Class(
                                baml_compiler2_hir_ty::layout::class_head(db, *class),
                                type_arg_templates
                                    .iter()
                                    .map(|t| t.substitute_symbolic(&[]))
                                    .collect(),
                            )),
                            Rvalue::Array(element, _) => {
                                Some(RuntimeTy::List(Box::new(element.substitute_symbolic(&[]))))
                            }
                            Rvalue::Map(key, value, _) => Some(RuntimeTy::Map {
                                key: Box::new(key.substitute_symbolic(&[])),
                                value: Box::new(value.substitute_symbolic(&[])),
                            }),
                            _ => None,
                        };
                        if let Some(ty) = ty {
                            plan.register_class_type(&ty)?;
                            plan.candidate(&ty);
                        }
                    }
                    if let StatementKind::Assign {
                        value: Rvalue::LoadType(t),
                        ..
                    } = &s.kind
                    {
                        plan.remember_heads(&t.substitute_symbolic(&[]));
                        if !plan.type_codes.contains(t) {
                            plan.type_codes.push(t.clone());
                        }
                    }
                }
                if b.terminator.as_ref().is_some_and(|term| {
                    crate::analysis::array_iter_walk(db, body, mir.arity, term, &iters).is_some()
                        || crate::analysis::array_next_call(db, term, &iters).is_some()
                }) {
                    continue;
                }
                match &b.terminator {
                    Some(Terminator::Call {
                        callee: Operand::Constant(Constant::Function(f)),
                        args,
                        ntypeargs,
                        ..
                    }) => {
                        if super::tasks::resource_concat(
                            db,
                            plan,
                            body,
                            b.terminator.as_ref().unwrap(),
                        ) {
                            continue;
                        }
                        let Some((callee, _)) = plan.index.function(db, *f) else {
                            continue;
                        };
                        if plan.index.display(callee) == "baml._to_string_default" {
                            let ty = operand_ty(plan, object, body, &args[*ntypeargs]);
                            if !matches!(
                                ty,
                                Some(
                                    RuntimeTy::Int
                                        | RuntimeTy::Float
                                        | RuntimeTy::Bool
                                        | RuntimeTy::String
                                        | RuntimeTy::Null
                                        | RuntimeTy::Literal(..)
                                )
                            ) {
                                plan.structural_strings = true;
                            }
                        }
                        if baml_builtins2_codegen::supports_rt(plan.index.display(callee))
                            || plan.index.display(callee) == "baml.json.decode"
                            || super::tasks::native(plan.index.display(callee))
                            || super::callbacks::native(plan.index.display(callee))
                        {
                            continue;
                        }
                        if *ntypeargs == 0 {
                            queue.push_back(callee);
                            continue;
                        }
                        let types = args[..*ntypeargs]
                            .iter()
                            .map(|o| type_operand(body, o))
                            .collect::<Gen<Vec<_>>>()?;
                        let id = instance(db, plan, all, &mut instances, callee, types)?;
                        plan.specialized_calls.insert((object, b.id.0), id);
                        queue.push_back(id);
                    }
                    Some(Terminator::VirtualCall {
                        iface,
                        method,
                        args,
                        ntypeargs,
                        self_arg,
                        ..
                    }) => {
                        plan.remember_heads(&iface.to_template().substitute_symbolic(&[]));
                        let method_args = args[..*ntypeargs]
                            .iter()
                            .map(|a| type_operand(body, a))
                            .collect::<Gen<Vec<_>>>()?;
                        let receiver =
                            operand_ty(plan, object, body, &args[*ntypeargs + *self_arg]);
                        let candidates = receiver
                            .as_ref()
                            .map(|t| match t {
                                RuntimeTy::Union(m) => m.to_vec(),
                                RuntimeTy::Interface(..)
                                | RuntimeTy::Unknown
                                | RuntimeTy::Function { .. } => plan.concrete_types.clone(),
                                _ => vec![t.clone()],
                            })
                            .unwrap_or_else(|| plan.concrete_types.clone());
                        let mut targets = Vec::new();
                        for concrete in candidates {
                            let Some((callee, frame)) = super::dispatch::resolve(
                                db,
                                plan,
                                &concrete,
                                iface,
                                method,
                                &method_args,
                            )?
                            else {
                                continue;
                            };
                            let target = instance(db, plan, all, &mut instances, callee, frame)?;
                            let repr = plan.repr(&concrete).map_err(|error| {
                                format!("{error} (interface receiver {concrete:?})")
                            })?;
                            if super::dispatch::structural_string_default(
                                db, plan, &concrete, iface, method,
                            )? {
                                plan.structural_string_calls.insert((object, b.id.0, repr));
                            }
                            if !targets.contains(&(repr, target)) {
                                targets.push((repr, target));
                            }
                            queue.push_back(target);
                        }
                        if !targets.is_empty() {
                            plan.dispatches.insert((object, b.id.0), targets);
                        }
                    }
                    _ => {}
                }
            }
        }
        super::formatting::register(db, plan, all, &mut instances, &mut queue)?;
        if !queue.is_empty() {
            continue;
        }
        if plan.concrete_types.len() == type_count {
            break;
        }
        queue.extend(seen.iter().copied());
        seen.clear();
    }
    plan.generic_functions = instances;
    Ok(())
}
// The lowerer emits a private bound-method temporary before specializing its
// callable-owned suffix. Fuse that one-use pair without constructing an open
// generic adapter. The receiver and its owner frame are preserved exactly.
fn specialize_bound_values(plan: &Plan<'_>, object: usize, mir: &mut MirFunction<'_>) -> Gen<()> {
    let MirFunctionKind::Bytecode(body) = &mir.kind else {
        return Ok(());
    };
    let mut replacements = Vec::new();
    for (bi, block) in body.blocks.iter().enumerate() {
        for (si, statement) in block.statements.iter().enumerate() {
            let StatementKind::Assign {
                value:
                    Rvalue::MakeGenericFunctionFromValue {
                        value:
                            Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)),
                        type_arg_templates,
                    },
                ..
            } = &statement.kind
            else {
                continue;
            };
            if body.locals[local.0].name.is_some() || body.locals[local.0].is_captured {
                continue;
            }
            let mut uses = 0;
            let mut count = |operand: &Operand<'_>| {
                if matches!(operand,Operand::Copy(Place::Local(l)) | Operand::Move(Place::Local(l)) if l==local)
                {
                    uses += 1;
                }
            };
            for b in &body.blocks {
                for s in &b.statements {
                    crate::operands::walk_statement_operands(&s.kind, &mut count);
                }
                if let Some(t) = &b.terminator {
                    crate::operands::walk_terminator_operands(t, &mut count);
                }
            }
            if uses != 1 {
                continue;
            }
            let Some((from, source)) =
                block.statements[..si]
                    .iter()
                    .enumerate()
                    .rev()
                    .find_map(|(i, s)| match &s.kind {
                        StatementKind::Assign {
                            destination: Place::Local(l),
                            value,
                        } if l == local => Some((i, value)),
                        _ => None,
                    })
            else {
                continue;
            };
            let value = match source {
                Rvalue::MakeBoundMethod {
                    func,
                    receiver,
                    frame,
                } => {
                    let mut prefix = match frame {
                        baml_compiler2_mir::BoundFrame::Explicit(types) => types.clone(),
                        baml_compiler2_mir::BoundFrame::Receiver => {
                            let types = match operand_ty(plan, object, body, receiver) {
                                Some(RuntimeTy::Class(_, args)) => args.to_vec(),
                                Some(RuntimeTy::List(element)) => vec![*element],
                                Some(RuntimeTy::Map { key, value }) => vec![*key, *value],
                                _ => vec![],
                            };
                            types
                                .into_iter()
                                .map(|t| {
                                    baml_compiler2_mir::RealizedTy::try_from(t)
                                        .map(TyTemplate::from)
                                        .map_err(|_| no("bound generic owner frame"))
                                })
                                .collect::<Gen<Vec<_>>>()?
                        }
                    };
                    prefix.extend(type_arg_templates.iter().cloned());
                    Rvalue::MakeBoundMethod {
                        func: *func,
                        receiver: receiver.clone(),
                        frame: baml_compiler2_mir::BoundFrame::Explicit(prefix),
                    }
                }
                Rvalue::MakeVirtualBoundMethod {
                    iface,
                    method,
                    receiver,
                    type_args,
                } => {
                    let mut args = type_args.clone();
                    args.extend(type_arg_templates.iter().cloned());
                    Rvalue::MakeVirtualBoundMethod {
                        iface: iface.clone(),
                        method: method.clone(),
                        receiver: receiver.clone(),
                        type_args: args,
                    }
                }
                _ => continue,
            };
            replacements.push((bi, from, si, value));
        }
    }
    let MirFunctionKind::Bytecode(body) = &mut mir.kind else {
        unreachable!()
    };
    for (bi, from, si, value) in replacements {
        body.blocks[bi].statements[from].kind = StatementKind::Nop;
        let StatementKind::Assign { value: target, .. } = &mut body.blocks[bi].statements[si].kind
        else {
            unreachable!()
        };
        *target = value;
    }
    Ok(())
}
pub(super) fn type_operand(body: &MirFunctionBody<'_>, operand: &Operand<'_>) -> Gen<RuntimeTy> {
    let (Operand::Copy(Place::Local(l)) | Operand::Move(Place::Local(l))) = operand else {
        return Err(no("non-static generic type argument"));
    };
    let mut found = None;
    for s in body.blocks.iter().flat_map(|b| &b.statements) {
        if let StatementKind::Assign {
            destination: Place::Local(dst),
            value: Rvalue::LoadType(t),
        } = &s.kind
        {
            if dst == l {
                let ty = t.substitute_symbolic(&[]);
                baml_compiler2_mir::RealizedTy::try_from(ty.clone())
                    .map_err(|_| no("non-concrete generic type argument"))?;
                if found.as_ref().is_some_and(|f| *f != ty) {
                    return Err(no("dynamic generic type argument"));
                }
                found = Some(ty);
            }
        }
    }
    found.ok_or_else(|| no("generic type definition missing"))
}
pub(super) fn specialize<'db>(
    mir: &MirFunction<'db>,
    params: &[baml_type::ParamTy],
    args: &[RuntimeTy],
    runtime: &baml_compiler2_mir::RuntimeLowering<'_>,
) -> Gen<MirFunction<'db>> {
    let mut mir = mir.clone();
    let template = |t: &TyTemplate| -> Gen<TyTemplate> {
        let substituted = t.substitute_symbolic(args);
        let reduced = baml_compiler2_hir_ty::package_interface::reduce_ground_projections(
            runtime.db,
            &baml_type::Ty::from(substituted),
            64,
        );
        let realized = baml_compiler2_mir::RealizedTy::try_from(runtime.convert(&reduced))
            .map_err(|_| no("unresolved generic projection"))?;
        Ok(TyTemplate::from(realized))
    };
    let test = |t: &mut TypeTest<'db>| -> Gen<()> {
        match t {
            TypeTest::Template(t) => *t = template(t)?,
            TypeTest::Class { args, .. } => {
                for t in args {
                    *t = template(t)?;
                }
            }
            _ => {}
        }
        Ok(())
    };
    let (erased_head, owner) = match &mir.identity {
        baml_compiler2_mir::MirFunctionId::Declared(f) => {
            let callable = baml_compiler2_hir::loc::DeclRef::Source(*f);
            let declared = baml_compiler2_hir_ty::lower::owner_self_ty(runtime.db, *f);
            let head = match &declared {
                Some(baml_type::Ty::Class(head, _)) => Some(head.clone()),
                _ => baml_compiler2_hir_ty::callable::callable_owner_type(runtime.db, callable),
            };
            let owner = declared
                .map(|t| {
                    baml_compiler2_mir::tir2_to_template(&t, runtime, params)
                        .substitute_symbolic(args)
                })
                .or_else(|| {
                    matches!(
                        baml_compiler2_hir::item_data::method_owner(runtime.db, *f),
                        Some(baml_compiler2_hir::item_data::MethodOwner::Interface(_))
                    )
                    .then(|| args.first().cloned())
                    .flatten()
                });
            (head, owner)
        }
        _ => (None, None),
    };
    if let MirFunctionKind::Bytecode(body) = &mut mir.kind {
        for l in &mut body.locals {
            // MIR erases impl self arguments and represents a default body's
            // self by its interface head. The VM restores Self from the frame;
            // native layouts must do the same at specialization time.
            if let (RuntimeTy::Class(head, erased), Some(owner_head), Some(owner_ty)) =
                (&l.ty, &erased_head, &owner)
            {
                if erased.is_empty() && head == owner_head {
                    l.ty = owner_ty.clone();
                    continue;
                }
            }
            let substituted = baml_compiler2_mir::tir2_to_template(
                &baml_type::Ty::from(l.ty.clone()),
                runtime,
                params,
            )
            .substitute_symbolic(args);
            l.ty = runtime.convert(
                &baml_compiler2_hir_ty::package_interface::reduce_ground_projections(
                    runtime.db,
                    &baml_type::Ty::from(substituted),
                    64,
                ),
            );
        }
        for b in &mut body.blocks {
            for s in &mut b.statements {
                if let StatementKind::Assign { value, .. } = &mut s.kind {
                    match value {
                        Rvalue::LoadType(t) | Rvalue::Array(t, _) => *t = template(t)?,
                        Rvalue::MakeBoundMethod {
                            frame: baml_compiler2_mir::BoundFrame::Explicit(types),
                            ..
                        } => {
                            for t in types {
                                *t = template(t)?;
                            }
                        }
                        Rvalue::MakeClosure {
                            type_arg_templates, ..
                        }
                        | Rvalue::MakeGenericFunction {
                            type_arg_templates, ..
                        }
                        | Rvalue::MakeGenericFunctionFromValue {
                            type_arg_templates, ..
                        } => {
                            for t in type_arg_templates {
                                *t = template(t)?;
                            }
                        }
                        Rvalue::Map(k, v, _) => {
                            *k = template(k)?;
                            *v = template(v)?;
                        }
                        Rvalue::Aggregate {
                            kind:
                                AggregateKind::Class {
                                    type_arg_templates, ..
                                },
                            ..
                        } => {
                            for t in type_arg_templates {
                                *t = template(t)?;
                            }
                        }
                        Rvalue::IsType { test: t, .. } => test(t)?,
                        Rvalue::MakeVirtualBoundMethod {
                            iface, type_args, ..
                        } => {
                            for ty in type_args {
                                *ty = template(ty)?;
                            }
                            for ty in &mut iface.generics {
                                *ty = template(ty)?;
                            }
                            for (_, ty) in &mut iface.associated_types {
                                *ty = template(ty)?;
                            }
                        }
                        Rvalue::MakeVirtualFunction { iface, self_ty, .. } => {
                            *self_ty = template(self_ty)?;
                            for ty in &mut iface.generics {
                                *ty = template(ty)?;
                            }
                            for (_, ty) in &mut iface.associated_types {
                                *ty = template(ty)?;
                            }
                        }
                        Rvalue::VirtualFieldAccess { iface, .. } => {
                            for t in &mut iface.generics {
                                *t = template(t)?;
                            }
                            for (_, t) in &mut iface.associated_types {
                                *t = template(t)?;
                            }
                        }
                        _ => {}
                    }
                }
            }
            for s in &mut b.statements {
                if let StatementKind::VirtualFieldStore { iface, .. } = &mut s.kind {
                    for t in &mut iface.generics {
                        *t = template(t)?;
                    }
                    for (_, t) in &mut iface.associated_types {
                        *t = template(t)?;
                    }
                }
            }
            match &mut b.terminator {
                Some(Terminator::VirtualCall { iface, .. }) => {
                    for t in &mut iface.generics {
                        *t = template(t)?;
                    }
                    for (_, t) in &mut iface.associated_types {
                        *t = template(t)?;
                    }
                }
                Some(Terminator::NarrowBind { test: t, .. }) => test(t)?,
                _ => {}
            }
        }
    }
    mir.lambdas = mir
        .lambdas
        .iter()
        .map(|lambda| specialize(lambda, params, args, runtime))
        .collect::<Gen<_>>()?;
    Ok(mir)
}
pub(super) struct Feature;
impl registry::Feature for Feature {
    fn terminator(&self, e: &mut Emitter<'_, '_>, term: &Terminator<'_>) -> Option<Gen<String>> {
        e.plan
            .specialized_calls
            .get(&(e.object, e.block))
            .map(|&target| {
                (|| {
                    let Terminator::Call {
                        args,
                        ntypeargs,
                        destination,
                        argument_layout,
                        ..
                    } = term
                    else {
                        return Err(no("generic call shape"));
                    };
                    let MirFunctionKind::Bytecode(body) = &e.all[&target].kind else {
                        return Err(no("generic builtin body"));
                    };
                    let mut values = args[*ntypeargs..]
                        .iter()
                        .map(|a| e.operand(a))
                        .collect::<Gen<Vec<_>>>()?;
                    if let Some(layout) = argument_layout {
                        let mapping = layout
                            .map_to(&e.plan.index.info(target).argument_layout())
                            .map_err(no)?;
                        values = mapping
                            .into_iter()
                            .map(|i| {
                                Ok(i.map(|i| values[i].clone()).unwrap_or_else(|| {
                                    ("rt::Dyn{tag:u64::MAX,payload:0}".into(), Repr::Tagged)
                                }))
                            })
                            .collect::<Gen<_>>()?;
                    }
                    let fast = super::tasks::fast_plan(e, term, &values)?;
                    let arguments = values
                        .into_iter()
                        .enumerate()
                        .map(|(i, v)| e.coerce(v, e.plan.local_repr(body, i + 1, 0)?))
                        .collect::<Gen<Vec<_>>>()?
                        .join(",");
                    let ordinary = e.assign(
                        destination,
                        (
                            e.fallible(&e.stack_leaf_call(
                                target,
                                &arguments,
                                &format!(
                                    "f{target}::<OBSERVE>(cx,{},{arguments})",
                                    e.telemetry_site(target)
                                ),
                            )),
                            e.plan.repr(&body.locals[0].ty)?,
                        ),
                    )?;
                    Ok(fast.map_or(ordinary.clone(), |fast| {
                        format!("if OBSERVE{{{ordinary}}}else{{{fast}}}")
                    }))
                })()
            })
    }
}

impl Plan<'_> {
    pub(super) fn class_origin(&self, c: usize) -> usize {
        self.class_instances
            .iter()
            .find(|(_, _, id)| *id == c)
            .map(|(c, _, _)| *c)
            .unwrap_or(c)
    }
    pub(super) fn class(&self, c: usize) -> &bex_vm_types::types::Class {
        let Object::Class(class) =
            &self.program.objects[bex_vm_types::ObjectIndex::from_raw(self.class_origin(c))]
        else {
            panic!("class metadata")
        };
        class
    }
    pub(super) fn class_field_types(&self, c: usize) -> Vec<bex_vm_types::RuntimeTy> {
        let class = self.class(c);
        if let Some((_, args, _)) = self.class_instances.iter().find(|(_, _, id)| *id == c) {
            class
                .fields
                .iter()
                .map(|f| f.field_template.substitute_symbolic(args))
                .collect()
        } else {
            class.fields.iter().map(|f| f.field_type.clone()).collect()
        }
    }
    pub(super) fn register_class_type(&mut self, ty: &RuntimeTy) -> Gen<()> {
        let Ok(realized) = baml_compiler2_mir::RealizedTy::try_from(ty.clone()) else {
            return Ok(());
        };
        let Some(anchored) = self.index.anchor(&TyTemplate::from(realized)) else {
            return Ok(());
        };
        self.register_field_class_type(&anchored.substitute_symbolic(&[]))
    }
    fn register_field_class_type(&mut self, ty: &bex_vm_types::RuntimeTy) -> Gen<()> {
        use bex_vm_types::RuntimeTy as T;
        match ty {
            T::Class(head, args) if !args.is_empty() => {
                let origin = self.class_tag(head.tag())?;
                if self
                    .class_instances
                    .iter()
                    .any(|(c, a, _)| *c == origin && a.as_slice() == args.as_ref())
                {
                    return Ok(());
                }
                if self.class_instances.len() > 4096 {
                    return Err(no("unbounded generic class layouts"));
                }
                let id = self.next_object;
                self.next_object += 1;
                self.class_instances.push((origin, args.to_vec(), id));
                for arg in args {
                    self.register_field_class_type(arg)?;
                }
                for field in self.class_field_types(id) {
                    self.register_field_class_type(&field)?;
                }
            }
            T::Union(members) => {
                for t in members {
                    self.register_field_class_type(t)?;
                }
            }
            T::List(t) => self.register_field_class_type(t)?,
            T::Map { key, value } => {
                self.register_field_class_type(key)?;
                self.register_field_class_type(value)?;
            }
            _ => {}
        }
        Ok(())
    }
}

pub(super) fn instance<'db>(
    db: &'db dyn Db,
    plan: &mut Plan<'_>,
    all: &mut BTreeMap<usize, MirFunction<'db>>,
    instances: &mut Vec<(usize, Vec<RuntimeTy>, usize)>,
    callee: usize,
    frame: Vec<RuntimeTy>,
) -> Gen<usize> {
    if frame.is_empty() {
        return Ok(callee);
    }
    if let Some((_, _, id)) = instances
        .iter()
        .find(|(c, t, _)| *c == callee && *t == frame)
    {
        return Ok(*id);
    }
    if instances.len() > 4096 {
        return Err(no("unbounded interface specialization"));
    }
    let source = all
        .get(&callee)
        .ok_or_else(|| no("interface method MIR missing"))?;
    let baml_compiler2_mir::MirFunctionId::Declared(f) = source.identity else {
        return Err(no("synthetic generic interface body"));
    };
    let params = callable_generic_frame(db, baml_compiler2_hir::loc::DeclRef::Source(f)).params;
    let runtime = baml_compiler2_mir::RuntimeLowering::of(db, f.file(db).source_root(db));
    let specialized = specialize(source, &params, &frame, &runtime)?;
    let runtime_params = baml_type::RuntimeGenericLayout::new(&params);
    if runtime_params.params().len() != frame.len() {
        return Err(no("specialization frame arity"));
    }
    let captures = runtime_params
        .params()
        .iter()
        .zip(&frame)
        .map(|(p, t)| (p.as_str().to_owned(), t.clone()))
        .collect();
    let id = plan.next_object;
    plan.next_object += 1;
    plan.function_frames.insert(id, captures);
    all.insert(id, specialized);
    plan.index.rt_alias(id, callee);
    instances.push((callee, frame, id));
    Ok(id)
}
fn operand_ty(
    plan: &Plan<'_>,
    object: usize,
    body: &MirFunctionBody<'_>,
    operand: &Operand<'_>,
) -> Option<RuntimeTy> {
    fn place(
        plan: &Plan<'_>,
        object: usize,
        body: &MirFunctionBody<'_>,
        p: &Place,
        depth: usize,
    ) -> Option<RuntimeTy> {
        if depth > body.locals.len() {
            return None;
        }
        match p {
            Place::Capture(i) | Place::Deref(baml_compiler2_mir::CellId::Capture(i)) => {
                plan.capture_types.get(&object)?.get(*i).cloned()
            }
            Place::Local(l) | Place::Deref(baml_compiler2_mir::CellId::Local(l)) => {
                let ty = &body.locals[l.0].ty;
                if body.locals[l.0].name.is_none()
                    && matches!(
                        ty,
                        RuntimeTy::Function { .. } | RuntimeTy::Unknown | RuntimeTy::Null
                    )
                {
                    for statement in body.blocks.iter().flat_map(|b| &b.statements) {
                        if let StatementKind::Assign {
                            destination: Place::Local(dst),
                            value: Rvalue::Use(Operand::Copy(from) | Operand::Move(from)),
                        } = &statement.kind
                        {
                            if dst == l {
                                if let Some(ty) = place(plan, object, body, from, depth + 1) {
                                    return Some(ty);
                                }
                            }
                        }
                    }
                }
                Some(ty.clone())
            }
            _ => None,
        }
    }
    match operand {
        Operand::Copy(p) | Operand::Move(p) => place(plan, object, body, p, 0),
        Operand::Constant(Constant::Int(_)) => Some(RuntimeTy::Int),
        Operand::Constant(Constant::Bool(_)) => Some(RuntimeTy::Bool),
        Operand::Constant(Constant::String(_)) => Some(RuntimeTy::String),
        Operand::Constant(Constant::Null) => Some(RuntimeTy::Null),
        _ => None,
    }
}
impl Plan<'_> {
    pub(super) fn remember_heads(&mut self, ty: &RuntimeTy) {
        ty.visit_heads(&mut |head| {
            if let Some(anchored) = self.index.anchor(&TyTemplate::TypeAlias(head.clone())) {
                if let bex_vm_types::TyTemplate::TypeAlias(h) = anchored {
                    self.heads.insert(h.tag(), head.clone());
                }
            }
        });
    }
    fn candidate(&mut self, ty: &RuntimeTy) {
        if matches!(ty,RuntimeTy::Class(head,args) if args.is_empty() && head.builtin_primitive(self.lang)==Some(baml_type::PrimitiveType::String))
        {
            self.candidate(&RuntimeTy::String);
            return;
        }
        if matches!(ty,RuntimeTy::Class(head,args) if args.is_empty() && head.builtin_primitive(self.lang)==Some(baml_type::PrimitiveType::Uint8Array))
        {
            self.candidate(&RuntimeTy::Uint8Array);
            return;
        }
        self.remember_heads(ty);
        match ty {
            RuntimeTy::Union(m) => {
                for t in m {
                    self.candidate(t);
                }
            }
            RuntimeTy::Class(_, args) => {
                for t in args {
                    self.candidate(t);
                }
                if !self.concrete_types.contains(ty) {
                    self.concrete_types.push(ty.clone());
                }
            }
            RuntimeTy::List(t) => {
                self.candidate(t);
                if !self.concrete_types.contains(ty) {
                    self.concrete_types.push(ty.clone());
                }
            }
            RuntimeTy::Map { key, value } => {
                self.candidate(key);
                self.candidate(value);
                if !self.concrete_types.contains(ty) {
                    self.concrete_types.push(ty.clone());
                }
            }
            RuntimeTy::Literal(baml_type::Literal::Int(_), _) => self.candidate(&RuntimeTy::Int),
            RuntimeTy::Literal(baml_type::Literal::Bool(_), _) => self.candidate(&RuntimeTy::Bool),
            RuntimeTy::Literal(baml_type::Literal::String(_), _) => {
                self.candidate(&RuntimeTy::String)
            }
            RuntimeTy::Int
            | RuntimeTy::Bigint
            | RuntimeTy::Bool
            | RuntimeTy::String
            | RuntimeTy::Null
            | RuntimeTy::Float
            | RuntimeTy::Uint8Array
            | RuntimeTy::Enum(_) => {
                if !self.concrete_types.contains(ty) {
                    self.concrete_types.push(ty.clone());
                }
            }
            _ => {}
        }
    }
}
