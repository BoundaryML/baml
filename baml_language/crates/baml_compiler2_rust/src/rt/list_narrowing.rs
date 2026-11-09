//! Preserve class facts along MIR edges so narrowed list fields keep their
//! concrete buffer representation. Joins intersect facts; writes invalidate them.
use super::*;
type Facts = BTreeMap<usize, BTreeSet<usize>>;
fn local(operand: &Operand<'_>) -> Option<usize> {
    match operand {
        Operand::Copy(Place::Local(l)) | Operand::Move(Place::Local(l)) => Some(l.0),
        _ => None,
    }
}
fn statement(facts: &mut Facts, kind: &StatementKind<'_>) {
    if let StatementKind::Assign {
        destination: Place::Local(dst),
        value,
    } = kind
    {
        let class = match value {
            Rvalue::Use(o) => local(o).and_then(|l| facts.get(&l).cloned()),
            _ => None,
        };
        facts.remove(&dst.0);
        if let Some(class) = class {
            facts.insert(dst.0, class);
        }
    }
}
pub(super) fn analyze(
    db: &dyn Db,
    plan: &Plan<'_>,
    body: &MirFunctionBody<'_>,
) -> BTreeMap<(usize, usize, usize), usize> {
    use baml_compiler2_mir::SwitchKey;
    fn classes(plan: &Plan<'_>, ty: &RuntimeTy) -> Option<BTreeSet<usize>> {
        if let RuntimeTy::Union(members) = ty {
            let mut all = BTreeSet::new();
            for member in members {
                all.extend(classes(plan, member)?);
            }
            Some(all)
        } else if matches!(ty, RuntimeTy::Class(_, _)) {
            if let Ok(Repr::Ptr(class)) = plan.repr(ty) {
                Some(BTreeSet::from([class]))
            } else {
                None
            }
        } else {
            None
        }
    }
    let universes = body
        .locals
        .iter()
        .map(|l| classes(plan, &l.ty))
        .collect::<Vec<_>>();
    let exclude = |facts: &mut Facts, subject: usize, excluded: &BTreeSet<usize>| {
        if let Some(mut possible) = facts
            .get(&subject)
            .cloned()
            .or_else(|| universes[subject].clone())
        {
            possible.retain(|c| !excluded.contains(c));
            facts.insert(subject, possible);
        }
    };
    let mut tags = BTreeMap::new();
    let mut tests = BTreeMap::new();
    let mut definitions = BTreeMap::<usize, usize>::new();
    for block in &body.blocks {
        for (at, s) in block.statements.iter().enumerate() {
            if let StatementKind::Assign {
                destination: Place::Local(dst),
                value,
            } = &s.kind
            {
                *definitions.entry(dst.0).or_default() += 1;
                if let Rvalue::TypeTag(Place::Local(src)) = value {
                    tags.insert(dst.0, (src.0, block.id.0, at));
                }
                if let Rvalue::IsType { operand, test } = value {
                    if let (Some(subject), Ok(Repr::Ptr(class))) =
                        (local(operand), plan.template_repr(&test.template(db)))
                    {
                        tests.insert(dst.0, (subject, class, block.id.0, at));
                    }
                }
            }
        }
        if let Some((Place::Local(dst), _)) = block
            .terminator
            .as_ref()
            .and_then(crate::analysis::continuation)
        {
            *definitions.entry(dst.0).or_default() += 1;
        }
    }
    // A saved type test describes the value read at that point, not a later
    // assignment to the same local. Only carry facts from a fresh test.
    let fresh = |block: &baml_compiler2_mir::BasicBlock<'_>,
                 subject: usize,
                 origin: usize,
                 at: usize| {
        block.id.0==origin && !block.statements[at+1..].iter().any(|s|matches!(&s.kind,StatementKind::Assign{destination:Place::Local(dst),..} if dst.0==subject))
    };
    tags.retain(|l, _| definitions.get(l) == Some(&1));
    tests.retain(|l, _| definitions.get(l) == Some(&1));
    let mut incoming = vec![None::<Facts>; body.blocks.len()];
    incoming[body.entry.0] = Some(Facts::new());
    let mut queue = std::collections::VecDeque::from([body.entry.0]);
    while let Some(at) = queue.pop_front() {
        let block = &body.blocks[at];
        let mut facts = incoming[at].clone().unwrap();
        for s in &block.statements {
            statement(&mut facts, &s.kind);
        }
        let Some(term) = &block.terminator else {
            continue;
        };
        if let Some((Place::Local(dst), _)) = crate::analysis::continuation(term) {
            facts.remove(&dst.0);
        }
        let mut edges = term
            .successors()
            .into_iter()
            .map(|b| (b.0, facts.clone()))
            .collect::<Vec<_>>();
        match term {
            Terminator::Branch { condition, .. } => {
                if let Some(&(subject, class, origin, definition)) =
                    local(condition).and_then(|l| tests.get(&l))
                {
                    if fresh(block, subject, origin, definition) {
                        edges[0].1.insert(subject, BTreeSet::from([class]));
                        exclude(&mut edges[1].1, subject, &BTreeSet::from([class]));
                    }
                }
            }
            Terminator::Switch {
                discriminant, arms, ..
            } => {
                if let Some((subject, _, _)) = local(discriminant)
                    .and_then(|l| tags.get(&l))
                    .copied()
                    .filter(|&(subject, origin, definition)| {
                        fresh(block, subject, origin, definition)
                    })
                {
                    let mut excluded = BTreeSet::new();
                    for (i, (key, _)) in arms.iter().enumerate() {
                        if let SwitchKey::Class(class) = key {
                            if let Some((class, _)) = plan.index.class(db, *class) {
                                excluded.insert(class);
                                edges[i].1.insert(subject, BTreeSet::from([class]));
                            }
                        }
                    }
                    exclude(&mut edges.last_mut().unwrap().1, subject, &excluded);
                }
            }
            Terminator::NarrowBind {
                source,
                test,
                destination,
                ..
            } => {
                if let Ok(Repr::Ptr(class)) = plan.template_repr(&test.template(db)) {
                    edges[0].1.insert(destination.0, BTreeSet::from([class]));
                    if let Some(subject) = local(source) {
                        edges[0].1.insert(subject, BTreeSet::from([class]));
                        exclude(&mut edges[1].1, subject, &BTreeSet::from([class]));
                    }
                }
            }
            _ => {}
        }
        for (target, mut edge) in edges {
            if target == body.entry.0 {
                edge.clear();
            }
            let changed = if let Some(previous) = &mut incoming[target] {
                let before = previous.clone();
                previous.retain(|l, possible| {
                    if let Some(other) = edge.get(l) {
                        possible.extend(other);
                        true
                    } else {
                        false
                    }
                });
                before != *previous
            } else {
                incoming[target] = Some(edge);
                true
            };
            if changed {
                queue.push_back(target);
            }
        }
    }
    let mut result = BTreeMap::new();
    for (at, block) in body.blocks.iter().enumerate() {
        let Some(mut facts) = incoming[at].take() else {
            continue;
        };
        for i in 0..=block.statements.len() {
            for (&local, possible) in &facts {
                if possible.len() == 1 {
                    result.insert((at, i, local), *possible.first().unwrap());
                }
            }
            if let Some(s) = block.statements.get(i) {
                statement(&mut facts, &s.kind);
            }
        }
    }
    result
}
impl Emitter<'_, '_> {
    pub(super) fn class_base(&self, place: &Place) -> Gen<(String, Repr)> {
        let value = self.place(place)?;
        if value.1 == Repr::Tagged {
            if let Place::Local(local) = place {
                if let Some(&class) =
                    self.list_narrowings
                        .get(&(self.block, self.statement, local.0))
                {
                    let repr = Repr::Ptr(class);
                    return Ok((self.coerce(value, repr)?, repr));
                }
            }
        }
        Ok(value)
    }
}
impl Plan<'_> {
    pub(super) fn list_field_repr(
        &self,
        body: &MirFunctionBody<'_>,
        place: &Place,
        block: usize,
        at: usize,
        depth: usize,
    ) -> Gen<Repr> {
        if let Place::Field { base, field } = place {
            let repr = self.list_field_repr(body, base, block, at, depth)?;
            let class = match (repr, base.as_ref()) {
                (Repr::Ptr(class), _) => class,
                (Repr::Tagged, Place::Local(local)) => *self
                    .list_narrowing_facts
                    .get(&(body as *const _ as usize))
                    .and_then(|facts| facts.get(&(block, at, local.0)))
                    .ok_or_else(|| no("unnarrowed list field receiver"))?,
                _ => return Err(no("list field receiver")),
            };
            return self.field_repr(
                self.class_field_types(class)
                    .get(*field)
                    .ok_or_else(|| no("list field index"))?,
            );
        }
        self.storage_repr(body, place, depth)
    }
}
