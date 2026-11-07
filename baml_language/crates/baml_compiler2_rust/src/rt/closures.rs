use super::{
    Gen, MirFunctionBody, Object, Operand, Place, Plan, Repr, RuntimeTy, Rvalue, StatementKind,
    Terminator, no, registry,
};

impl Plan<'_> {
    // Infer copy-assigned receiver storage; all definitions must agree.
    // Actual function values, cycles, parameters and call results stay rejected.
    pub(super) fn local_repr(
        &self,
        body: &MirFunctionBody<'_>,
        i: usize,
        depth: usize,
    ) -> Gen<Repr> {
        let definitions: Vec<_> = body
            .blocks
            .iter()
            .flat_map(|b| &b.statements)
            .filter_map(|s| match &s.kind {
                StatementKind::Assign {
                    destination: Place::Local(l),
                    value,
                } if l.0 == i => Some(value),
                _ => None,
            })
            .collect();
        if definitions.iter().any(|v| matches!(v, Rvalue::LoadType(_))) {
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
            for value in definitions {
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
        if depth >= body.locals.len() || i == 0 || body.locals[i].name.is_some() {
            return Err(no("function-valued local"));
        }
        let mut inferred = None;
        for block in &body.blocks {
            for statement in &block.statements {
                if let StatementKind::Assign {
                    destination: Place::Local(dst),
                    value,
                } = &statement.kind
                    && dst.0 == i
                {
                    let Rvalue::Use(Operand::Copy(place) | Operand::Move(place)) = value else {
                        return Err(no("function-valued local definition"));
                    };
                    let repr = self.storage_repr(body, place, depth + 1)?;
                    if inferred.is_some_and(|previous| previous != repr) {
                        return Err(no("inconsistent receiver storage"));
                    }
                    inferred = Some(repr);
                }
            }
            if matches!(&block.terminator,
                Some(Terminator::Call { destination: Place::Local(dst), .. }
                    | Terminator::VirtualCall { destination: Place::Local(dst), .. }) if dst.0 == i)
            {
                return Err(no("function-valued call result"));
            }
        }
        inferred.ok_or_else(|| no("unresolved function-valued storage"))
    }
    pub(super) fn storage_repr(
        &self,
        body: &MirFunctionBody<'_>,
        place: &Place,
        depth: usize,
    ) -> Gen<Repr> {
        match place {
            Place::Local(local) => self.local_repr(body, local.0, depth),
            Place::Field { base, field } => {
                let Repr::Ptr(class) = self.storage_repr(body, base, depth)? else {
                    return Err(no("receiver field on non-class"));
                };
                let Object::Class(class) =
                    &self.program.objects[bex_vm_types::ObjectIndex::from_raw(class)]
                else {
                    return Err(no("receiver class"));
                };
                self.field_repr(
                    &class
                        .fields
                        .get(*field)
                        .ok_or_else(|| no("receiver field"))?
                        .field_type,
                )
            }
            _ => Err(no("receiver storage place")),
        }
    }
}

pub(super) struct Feature;
impl registry::Feature for Feature {
    fn admit(&self, event: &registry::Admission<'_, '_>) -> Gen<()> {
        match event {
            registry::Admission::Function(mir) if !mir.lambdas.is_empty() => Err(no("closures")),
            registry::Admission::CapturedLocal(true) => Err(no("captured local")),
            registry::Admission::Class { generic: true, .. } => Err(no("generic class or cleanup")),
            _ => Ok(()),
        }
    }
}
