//! Closed-world specializations. Each concrete call gets ordinary typed MIR;
//! source coordinates continue to refer to the original declaration.
use super::*;
use baml_compiler2_hir_ty::callable::callable_generic_frame;
use baml_compiler2_mir::{TyTemplate, TypeTest};

// Cache keys must identify types, not the number of times an associated error
// union has been substituted. In particular (never | never) | never is never.
pub(super) fn canonical_type<N: Clone + PartialEq>(
    ty: &baml_type::RuntimeTy<N>,
) -> baml_type::RuntimeTy<N> {
    ty.map_children(&mut canonical_type).flatten_union()
}

pub(super) fn has_projection<N: Clone>(ty: &baml_type::RuntimeTy<N>) -> bool {
    let mut found = matches!(ty, baml_type::RuntimeTy::AssociatedTypeProjection { .. });
    ty.visit_children(&mut |child| found |= has_projection(child));
    found
}

#[cfg(test)]
mod tests {
    use super::canonical_type;
    use baml_type::{Name, RuntimeTy as T, TypeName};

    #[test]
    fn specialization_keys_flatten_unions_inside_type_arguments() {
        let head = TypeName::local(Name::new("Iterator"));
        let ty = T::List(Box::new(T::Class(
            head.clone(),
            vec![T::Union(
                vec![T::Never, T::Union(vec![T::Never].into())].into(),
            )]
            .into(),
        )));
        let expected = T::List(Box::new(T::Class(head, vec![T::Never].into())));
        assert_eq!(canonical_type(&ty), expected);
        assert_eq!(canonical_type(&expected), expected);
    }
    #[test]
    fn full_and_dense_callback_frames_share_a_key_without_erasing_real_never() {
        use baml_type::{Name, ParamTy, RuntimeTy as T, SCOPED_PARAM_BIT};
        let params = [
            ParamTy::new(0, Name::new("T")),
            ParamTy::new(1, Name::new("__effect_param_0")),
            ParamTy::new(SCOPED_PARAM_BIT | 2, Name::new("__effect_param_0")),
        ];
        let full = vec![
            T::Union(vec![T::Int, T::Union(vec![T::Int].into())].into()),
            T::Never,
            T::Never,
        ];
        let dense = vec![T::Int, T::Never];
        assert_eq!(super::runtime_frame(&params, full), dense);
        assert_eq!(super::runtime_frame(&params, dense.clone()), dense);
    }
}

pub(super) fn expand<'db>(
    db: &'db dyn Db,
    plan: &mut Plan<'_>,
    all: &mut BTreeMap<usize, super::TestMir<'db>>,
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
            specialize_bound_values(plan, object, std::rc::Rc::make_mut(&mut mir))?;
            all.insert(object, mir.clone());
            let MirFunctionKind::Bytecode(body) = &mir.kind else {
                continue;
            };
            // These checks need no specialization. Reject a reachable body
            // before expanding its (possibly recursive) generic dependencies.
            registry::admit(registry::Admission::Function(&mir))?;
            for local in &body.locals {
                registry::admit(registry::Admission::CapturedLocal(local.is_captured))?;
            }
            super::exceptions::prepare_contexts(db, plan, body)?;
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
                        .lambda(
                            db,
                            lambda
                                .identity
                                .as_ref()
                                .ok_or_else(|| no("lambda identity"))?,
                        )
                        .ok_or_else(|| no("unlinked generic lambda"))?;
                    let target = if plan.function_frames.contains_key(&object) {
                        let target = plan.next_object;
                        plan.next_object += 1;
                        std::rc::Rc::make_mut(&mut plan.index).rt_alias(target, origin);
                        plan.function_frames
                            .insert(target, plan.function_frames[&object].clone());
                        all.insert(target, std::rc::Rc::new(lambda.clone()));
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
                        Constant::Bigint(_) => Some(RuntimeTy::Bigint),
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
                // Erased assertion equality and thrown enum values need capture
                // tags even when a literal never occupies an enum-typed local.
                if matches!(t, RuntimeTy::Enum(_)) {
                    plan.record_ty(&t)?;
                }
            }
            for (repr, method, callee, frame) in super::map_keys::discover(db, plan, body)? {
                let target = instance(db, plan, all, &mut instances, callee, frame)?;
                plan.key_callbacks
                    .entry(repr)
                    .or_default()
                    .insert(method, target);
                plan.key_edges.entry(object).or_default().insert(target);
                queue.push_back(target);
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
                            if super::map_keys::virtual_method(plan, b.terminator.as_ref().unwrap())
                                .is_some()
                            {
                                continue;
                            }
                            plan.remember_heads(&iface.to_template().substitute_symbolic(&[]));
                            let candidates = dispatch_candidates(plan, receiver.as_ref());
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
                                    let _ = plan.register_class_type(&concrete);
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
                        // Reflected type values of generic classes need their
                        // instantiated layouts even when never constructed. A
                        // class that cannot be laid out stays unsupported
                        // wherever its layout is actually required.
                        let _ = plan.register_class_type(&t.substitute_symbolic(&[]));
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
                        // Join's structural elements use the same nested ToString
                        // witnesses as string.from. Link overrides before admission.
                        if plan.index.display(callee) == "baml.Array.join" {
                            let element = operand_ty(plan, object, body, &args[*ntypeargs])
                                .and_then(|ty| plan.repr(&ty).ok())
                                .and_then(|repr| plan.element(repr).ok());
                            if !matches!(
                                element,
                                Some(Repr::Int | Repr::Float | Repr::Bool | Repr::Str)
                            ) {
                                plan.structural_strings = true;
                            }
                        }
                        if matches!(
                            plan.index.display(callee),
                            "baml._equals_structural_default" | "baml.ops.equals_equals"
                        ) {
                            plan.structural_equals = true;
                        }
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
                            || lists::handles(plan.index.display(callee))
                            || super::list_callbacks::handles(plan.index.display(callee))
                            || super::list_scalars::handles(plan.index.display(callee))
                            || plan.index.display(callee).starts_with("baml.Map.")
                            || matches!(
                                plan.index.display(callee),
                                "baml.json.decode"
                                    | "baml.json.stringify"
                                    | "baml.json._to"
                                    | "baml.json.to"
                                    | "baml.Array.filled"
                            )
                            || plan.index.display(callee) == "baml.json.decode"
                            || super::tasks::native(plan.index.display(callee))
                            || super::callbacks::native(plan.index.display(callee))
                            || super::reflection::native(plan.index.display(callee))
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
                        if super::map_keys::virtual_method(plan, b.terminator.as_ref().unwrap())
                            .is_some()
                        {
                            continue;
                        }
                        plan.remember_heads(&iface.to_template().substitute_symbolic(&[]));
                        let method_args = args[..*ntypeargs]
                            .iter()
                            .map(|a| type_operand(body, a))
                            .collect::<Gen<Vec<_>>>()?;
                        let receiver =
                            operand_ty(plan, object, body, &args[*ntypeargs + *self_arg]);
                        let candidates = dispatch_candidates(plan, receiver.as_ref());
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
                            let _ = plan.register_class_type(&concrete);
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
                        // A static interface receiver can only hold a closed-world
                        // concrete value. With no implementor constructed, the
                        // call is unreachable (e.g. a match arm only reached by null).
                        if !targets.is_empty() || interface_receiver(receiver.as_ref()) {
                            plan.dispatches.insert((object, b.id.0), targets);
                        }
                    }
                    _ => {}
                }
            }
        }
        super::formatting::register(db, plan, all, &mut instances, &mut queue)?;
        super::equality::register(db, plan, all, &mut instances, &mut queue)?;
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
        let realized =
            baml_compiler2_mir::RealizedTy::try_from(canonical_type(&runtime.convert(&reduced)))
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
        Some(baml_compiler2_mir::MirFunctionId::Declared(f)) => {
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
            l.ty = canonical_type(&runtime.convert(
                &baml_compiler2_hir_ty::package_interface::reduce_ground_projections(
                    runtime.db,
                    &baml_type::Ty::from(substituted),
                    64,
                ),
            ));
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
    pub(super) fn resolve_type(&self, ty: &RuntimeTy) -> RuntimeTy {
        if !has_projection(ty) {
            return ty.clone();
        }
        let reduced = baml_compiler2_hir_ty::package_interface::reduce_ground_projections(
            self.db,
            &baml_type::Ty::from(ty.clone()),
            64,
        );
        baml_compiler2_mir::RuntimeLowering::of(self.db, self.root).convert(&reduced)
    }
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
        let types =
            if let Some((_, args, _)) = self.class_instances.iter().find(|(_, _, id)| *id == c) {
                class
                    .fields
                    .iter()
                    .map(|f| f.field_template.substitute_symbolic(args))
                    .collect::<Vec<_>>()
            } else {
                class
                    .fields
                    .iter()
                    .map(|f| f.field_type.clone())
                    .collect::<Vec<_>>()
            };
        types.iter().map(|ty| self.resolve_field_type(ty)).collect()
    }

    fn resolve_field_type(&self, ty: &bex_vm_types::RuntimeTy) -> bex_vm_types::RuntimeTy {
        if !has_projection(ty) {
            return ty.clone();
        }
        let Ok(named) = ty.try_map_heads(&mut |head| self.index.declaration(head.tag()).ok_or(()))
        else {
            return ty.clone();
        };
        let reduced = baml_compiler2_hir_ty::package_interface::reduce_ground_projections(
            self.db,
            &baml_type::Ty::from(named),
            64,
        );
        let runtime = baml_compiler2_mir::RuntimeLowering::of(self.db, self.root);
        let Ok(realized) = baml_compiler2_mir::RealizedTy::try_from(runtime.convert(&reduced))
        else {
            return ty.clone();
        };
        self.index
            .anchor(&TyTemplate::from(realized))
            .map(|t| t.substitute_symbolic(&[]))
            .unwrap_or_else(|| ty.clone())
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
    pub(super) fn register_field_class_type(&mut self, ty: &bex_vm_types::RuntimeTy) -> Gen<()> {
        use bex_vm_types::RuntimeTy as T;
        type Origins = baml_type::template::TyTemplateOrigins<bex_vm_types::TypeHead>;
        enum Work {
            Visit(T, Origins),
            Leave,
        }
        // Explicit DFS keeps recursive layouts off the compiler's Rust stack.
        let mut pending = vec![Work::Visit(ty.clone(), Origins::root())];
        let mut ancestry: Vec<(bex_vm_types::TypeHead, usize)> = Vec::new();
        while let Some(work) = pending.pop() {
            let Work::Visit(ty, origins) = work else {
                ancestry.pop();
                continue;
            };
            let ty = self.resolve_field_type(&ty);
            match ty {
                T::Class(head, args) if !args.is_empty() => {
                    let origin = self.class_tag(head.tag())?;
                    // Union-only transforms close: (T | null) | null = T | null.
                    let args = args
                        .into_vec()
                        .into_iter()
                        .map(|ty| canonical_type(&ty))
                        .collect::<Vec<_>>();
                    if self
                        .class_instances
                        .iter()
                        .any(|(c, a, _)| *c == origin && *a == args)
                    {
                        continue;
                    }
                    for (index, (ancestor, arity)) in ancestry.iter().enumerate() {
                        if *ancestor == head
                            && origins.class_transform_expands(index, &head, *arity)
                        {
                            return Err(no("non-regular recursive generic class layouts"));
                        }
                    }
                    if self.class_instances.len() >= 4096 {
                        return Err(no("generic class layout limit (4096)"));
                    }
                    let id = self.next_object;
                    self.next_object += 1;
                    self.class_instances.push((origin, args.clone(), id));
                    ancestry.push((head, args.len()));
                    pending.push(Work::Leave);
                    for field in self.class(origin).fields.iter().rev() {
                        pending.push(Work::Visit(
                            field.field_template.substitute_symbolic(&args),
                            origins.through_field(&head, args.len(), &field.field_template),
                        ));
                    }
                    for (index, arg) in args.into_iter().enumerate().rev() {
                        pending.push(Work::Visit(
                            arg,
                            origins.through_field(
                                &head,
                                self.class(origin).generic_param_count,
                                &bex_vm_types::TyTemplate::TypeArgRef(index as u32),
                            ),
                        ));
                    }
                }
                T::Union(members) => {
                    for (index, member) in members.into_vec().into_iter().enumerate().rev() {
                        pending.push(Work::Visit(member, origins.union_member(index)));
                    }
                }
                T::List(element) => pending.push(Work::Visit(*element, origins.list_element())),
                T::Map { key, value } => {
                    pending.push(Work::Visit(*value, origins.map_value()));
                    pending.push(Work::Visit(*key, origins.map_key()));
                }
                // A non-recursive alias is laid out as its definition.
                T::TypeAlias(head) => {
                    if let Some(target) = self.alias_target(head.tag()) {
                        pending.push(Work::Visit(target, Origins::root()));
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

// Dynamic receivers, including interface members of optional or union
// receivers, may hold any closed-world concrete value.
fn dispatch_candidates(plan: &Plan<'_>, receiver: Option<&RuntimeTy>) -> Vec<RuntimeTy> {
    let mut candidates = Vec::new();
    let mut push = |ty: &RuntimeTy| {
        if !candidates.contains(ty) {
            candidates.push(ty.clone());
        }
    };
    let members = match receiver {
        Some(RuntimeTy::Union(members)) => members.to_vec(),
        Some(ty) => vec![ty.clone()],
        None => vec![RuntimeTy::Unknown],
    };
    for member in &members {
        match member {
            RuntimeTy::Interface(..) | RuntimeTy::Unknown | RuntimeTy::Function { .. } => {
                plan.concrete_types.iter().for_each(&mut push)
            }
            _ => push(member),
        }
    }
    candidates
}

fn interface_receiver(receiver: Option<&RuntimeTy>) -> bool {
    match receiver {
        Some(RuntimeTy::Interface(..)) => true,
        Some(RuntimeTy::Union(members)) => {
            members.iter().any(|m| matches!(m, RuntimeTy::Interface(..)))
                && members
                    .iter()
                    .all(|m| matches!(m, RuntimeTy::Interface(..) | RuntimeTy::Null))
        }
        _ => false,
    }
}

pub(super) fn runtime_frame(
    params: &[baml_type::ParamTy],
    frame: Vec<RuntimeTy>,
) -> Vec<RuntimeTy> {
    let layout = baml_type::RuntimeGenericLayout::new(params);
    let full = frame.len() == params.len();
    frame
        .into_iter()
        .enumerate()
        .filter_map(|(i, ty)| {
            if full && layout.slot(&params[i]).is_none() {
                None
            } else {
                Some(canonical_type(&ty))
            }
        })
        .collect()
}

pub(super) fn callable_frame(
    db: &dyn Db,
    function: baml_compiler2_hir_ty::extern_loc::FunctionRef<'_>,
    frame: Vec<RuntimeTy>,
) -> Vec<RuntimeTy> {
    runtime_frame(&callable_generic_frame(db, function).params, frame)
}

pub(super) fn instance<'db>(
    db: &'db dyn Db,
    plan: &mut Plan<'_>,
    all: &mut BTreeMap<usize, super::TestMir<'db>>,
    instances: &mut Vec<(usize, Vec<RuntimeTy>, usize)>,
    callee: usize,
    frame: Vec<RuntimeTy>,
) -> Gen<usize> {
    let source = all
        .get(&callee)
        .ok_or_else(|| no("interface method MIR missing"))?;
    let params = match source.identity {
        Some(baml_compiler2_mir::MirFunctionId::Declared(f)) => {
            callable_generic_frame(db, baml_compiler2_hir::loc::DeclRef::Source(f))
                .params
                .into_owned()
        }
        _ if frame.is_empty() => return Ok(callee),
        _ => return Err(no("synthetic generic interface body")),
    };
    let runtime_params = baml_type::RuntimeGenericLayout::new(&params);
    // Calls and materialized callable values must use the same dense frame key.
    let frame = runtime_frame(&params, frame);
    if frame.is_empty() {
        return Ok(callee);
    }
    if runtime_params.params().len() != frame.len() {
        return Err(no(format!(
            "specialization frame arity (emitting {}: expected {}, got {})",
            plan.index.display(callee),
            runtime_params.params().len(),
            frame.len()
        )));
    }
    if frame
        .iter()
        .any(|ty| baml_compiler2_mir::RealizedTy::try_from(ty.clone()).is_err())
    {
        return Err(no("non-concrete specialization argument"));
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
    let source = &all[&callee];
    // Include aliases declared in the user pack when specializing stdlib MIR.
    let runtime = baml_compiler2_mir::RuntimeLowering::of(db, plan.root);
    let mut specialized = specialize(source, &params, &frame, &runtime)?;
    fold_primitive_sort_guards(db, plan, &mut specialized);
    let captures = runtime_params
        .params()
        .iter()
        .zip(&frame)
        .map(|(p, t)| (p.as_str().to_owned(), t.clone()))
        .collect();
    // Builtins in a specialized body receive their frame as type tokens.
    for ty in &frame {
        if let Ok(realized) = baml_compiler2_mir::RealizedTy::try_from(ty.clone()) {
            let template = TyTemplate::from(realized);
            if !plan.type_codes.contains(&template) {
                plan.type_codes.push(template);
            }
        }
    }
    let id = plan.next_object;
    plan.next_object += 1;
    plan.function_frames.insert(id, captures);
    all.insert(id, std::rc::Rc::new(specialized));
    std::rc::Rc::make_mut(&mut plan.index).rt_alias(id, callee);
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
    pub(super) fn candidate(&mut self, ty: &RuntimeTy) {
        let normalized = canonical_type(ty);
        let ty = &normalized;
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
        if matches!(ty, RuntimeTy::Class(head, args) if args.is_empty() && head.is_lang_root_type(self.lang, baml_base::LangPackage::Reflect, "Type"))
        {
            self.candidate(&RuntimeTy::Type);
            return;
        }
        self.remember_heads(ty);
        // Dynamic/local type bindings are not closed-world witness candidates.
        if baml_compiler2_mir::RealizedTy::try_from(ty.clone()).is_err() {
            return;
        }
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
                // A closed-world candidate must have a layout to be
                // represented; register instantiations seen only as types.
                if !args.is_empty() && self.register_class_type(ty).is_err() {
                    return;
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
            RuntimeTy::Bigint => {
                self.bigints = true;
                if !self.concrete_types.contains(ty) {
                    self.concrete_types.push(ty.clone());
                }
            }
            RuntimeTy::Type
            | RuntimeTy::Int
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

// A homogeneous primitive list always takes the native natural-sort branch.
// Preserve the guard call (and its source site), but discard the impossible
// comparator closure before reachability and closure admission. Block numbers
// stay intact because error evidence refers to the original MIR coordinates.
fn fold_primitive_sort_guards(db: &dyn Db, plan: &Plan<'_>, mir: &mut MirFunction<'_>) {
    let MirFunctionKind::Bytecode(body) = &mut mir.kind else {
        return;
    };
    let mut guards = Vec::new();
    for block in &body.blocks {
        let Some(Terminator::Call {
            callee: Operand::Constant(Constant::Function(f)),
            args,
            ntypeargs,
            destination: Place::Local(result),
            target,
            ..
        }) = &block.terminator
        else {
            continue;
        };
        if !plan
            .index
            .function(db, *f)
            .is_some_and(|(id, _)| plan.index.display(id) == "baml._is_primitive_array")
        {
            continue;
        }
        let Some(Operand::Copy(Place::Local(list)) | Operand::Move(Place::Local(list))) =
            args.get(*ntypeargs)
        else {
            continue;
        };
        if matches!(&body.locals[list.0].ty,RuntimeTy::List(t) if matches!(t.as_ref(), RuntimeTy::Int|RuntimeTy::Float|RuntimeTy::String))
        {
            guards.push((*result, *target));
        }
    }
    if guards.is_empty() {
        return;
    }
    for (result, target) in guards {
        let block = &mut body.blocks[target.0];
        if block.statements.is_empty() {
            if let Some(Terminator::Branch {
                condition: Operand::Copy(Place::Local(l)) | Operand::Move(Place::Local(l)),
                then_block,
                ..
            }) = &block.terminator
            {
                if *l == result {
                    block.terminator = Some(Terminator::Goto {
                        target: *then_block,
                    });
                }
            }
        }
    }
    let mut reached = BTreeSet::new();
    let mut pending = vec![body.entry];
    while let Some(id) = pending.pop() {
        if !reached.insert(id.0) {
            continue;
        }
        let block = &body.blocks[id.0];
        pending.extend(block.unwind);
        if let Some(term) = &block.terminator {
            pending.extend(term.successors());
        }
    }
    for block in &mut body.blocks {
        if !reached.contains(&block.id.0) {
            // Erased type locals still need their concrete metadata for the
            // Rust local declarations; LoadType has no runtime side effect.
            block.statements.retain(|s| {
                matches!(
                    &s.kind,
                    StatementKind::Assign {
                        value: Rvalue::LoadType(_),
                        ..
                    }
                )
            });
            block.terminator = Some(Terminator::Unreachable);
            block.unwind = None;
        }
    }
}
