//! Language exceptions share layouts and tags with runtime-produced failures.
use super::*;

pub(super) fn context_type(traced: bool) -> &'static str {
    if traced {
        "rt::calls::ErrorContext"
    } else {
        "rt::calls::Context"
    }
}

pub(super) fn prepare_contexts(
    db: &dyn Db,
    plan: &mut Plan<'_>,
    body: &MirFunctionBody<'_>,
) -> Gen<()> {
    fn base(p: &Place) -> Option<usize> {
        match p {
            Place::Local(l) => Some(l.0),
            Place::Field { base: p, .. } | Place::Index { base: p, .. } => base(p),
            _ => None,
        }
    }
    let locals = body
        .blocks
        .iter()
        .filter_map(|b| b.landing.map(|l| l.context_local.0))
        .collect::<BTreeSet<_>>();
    let mut used = false;
    let mut operand = |o: &Operand<'_>| {
        if let Operand::Copy(p) | Operand::Move(p) = o {
            used |= base(p).is_some_and(|i| locals.contains(&i));
        }
    };
    for b in &body.blocks {
        for s in &b.statements {
            crate::operands::walk_statement_operands(&s.kind, &mut operand);
        }
        if let Some(t) = &b.terminator {
            if !matches!(t, Terminator::Rethrow { .. }) {
                crate::operands::walk_terminator_operands(t, &mut operand);
            }
        }
    }
    if used {
        plan.error_contexts = true;
        let class = baml_compiler2_hir_ty::layout::lang_class(
            db,
            baml_base::LangPackage::Baml,
            &["errors"],
            "Context",
        )
        .ok_or_else(|| no("Context declaration"))?;
        plan.candidate(&RuntimeTy::Class(
            baml_compiler2_hir_ty::layout::class_head(db, class),
            Box::new([]),
        ));
        for name in [
            "baml.errors.Context",
            "baml.errors.StackTrace",
            "baml.errors.StackFrame",
        ] {
            let c = super::strings::builtin_class(plan, name)?;
            plan.add(c)?;
        }
    }
    Ok(())
}

pub(super) fn schema(e: &Emitter<'_, '_>) -> Gen<String> {
    let c = super::strings::builtin_class(e.plan, "baml.errors.Context")?;
    let t = super::strings::builtin_class(e.plan, "baml.errors.StackTrace")?;
    let f = super::strings::builtin_class(e.plan, "baml.errors.StackFrame")?;
    Ok(format!(
        "rt::exceptions::ContextSchema{{context:{},trace:{},frame:{},frames:{}}}",
        e.descriptors[&c],
        e.descriptors[&t],
        e.descriptors[&f],
        e.lists[&Repr::ListPtr(f)]
    ))
}

pub(super) fn dividend<N: Clone + PartialEq>(ty: &baml_type::RuntimeTy<N>) -> bool {
    use baml_type::RuntimeTy as T;
    matches!(ty,T::Union(m) if m.len()==2 && m.contains(&T::Int) && m.contains(&T::Bigint))
}

pub(super) fn record_test(
    db: &dyn Db,
    plan: &mut Plan<'_>,
    test: &baml_compiler2_mir::TypeTest<'_>,
) -> Gen<()> {
    let ty = test.template(db).substitute_symbolic(&[]);
    if matches!(ty, RuntimeTy::Class(..)) {
        plan.record_ty(&ty)?;
    }
    Ok(())
}

pub(super) fn reserved(plan: &Plan<'_>, object: usize) -> Option<usize> {
    let class = plan.class(object);
    if !plan
        .program
        .packages
        .iter()
        .any(|p| p.name == "baml" && p.objects.contains(&(object as u32)))
    {
        return None;
    }
    Some(match class.path.to_string().as_str() {
        "panics.IntegerOverflow" => 1,
        "panics.NegativeBitShift" => 2,
        "panics.Cancelled" => 3,
        "panics.StackOverflow" => 4,
        "panics.AllocFailure" => 5,
        "panics.IndexOutOfBounds" => 6,
        "json.ParseError" => 7,
        "json.DecodeError" => 8,
        "panics.DivisionByZero" => 9,
        _ => return None,
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
            ..
        }) = term
        else {
            return None;
        };
        let (id, _) = plan.index.function(db, *f)?;
        matches!(
            plan.index.display(id),
            "baml.errors.Context._to_string_impl" | "baml.errors.StackTrace._to_string_impl"
        )
        .then(|| Ok(registry::Reach::default()))
    }
    fn call(
        &self,
        e: &Emitter<'_, '_>,
        name: &str,
        _: &[Repr],
        values: &[(String, Repr)],
    ) -> Option<Gen<(String, Repr)>> {
        if !matches!(
            name,
            "baml.errors.Context._to_string_impl" | "baml.errors.StackTrace._to_string_impl"
        ) {
            return None;
        }
        Some(if values.len() == 1 {
            Ok((
                e.fallible(&format!(
                    "rt::exceptions::to_string({}, {}, CAPTURE_SHAPES)",
                    values[0].0,
                    name.contains("Context")
                )),
                Repr::Str,
            ))
        } else {
            Err(no("error context renderer arity"))
        })
    }
    fn terminator(&self, e: &mut Emitter<'_, '_>, term: &Terminator<'_>) -> Option<Gen<String>> {
        let Terminator::Throw { value } = term else {
            return None;
        };
        Some((|| {
            let (value, repr) = e.operand(value)?;
            let value = e.capture_value(value, repr)?;
            let mut classes = BTreeMap::new();
            for (desc, name) in [
                "baml.panics.IntegerOverflow",
                "baml.panics.NegativeBitShift",
                "baml.panics.Cancelled",
                "baml.panics.StackOverflow",
                "baml.panics.AllocFailure",
                "baml.panics.IndexOutOfBounds",
                "baml.json.ParseError",
                "baml.json.DecodeError",
                "baml.panics.DivisionByZero",
            ]
            .iter()
            .enumerate()
            {
                classes.insert(desc + 4, (*name).to_owned());
            }
            for (&c, &desc) in e.descriptors {
                classes.insert(desc + 3, class_name(e.plan, c)?);
            }
            let mut names = String::new();
            for (tag, name) in classes {
                write!(names, "{tag}=>{name:?},").unwrap();
            }
            let code = format!(
                "{{let value={value};let class=match value.tag{{{names}_=>\"value\"}};rt::exceptions::thrown(value,class,CAPTURE_SHAPES)}}"
            );
            let error = e.error_at(&code);
            Ok(e.raise(&error))
        })())
    }
}

fn class_name(plan: &Plan<'_>, object: usize) -> Gen<String> {
    let origin = plan.class_origin(object);
    let package = plan
        .program
        .packages
        .iter()
        .find(|p| p.objects.contains(&(origin as u32)))
        .ok_or_else(|| no("throw class package"))?;
    Ok(format!("{}.{}", package.name, plan.class(object).path))
}
