//! Scalar replacement of class instances that never leave their function.
//!
//! An instance the function builds, reads and writes field by field, and
//! hands to nothing else, is a bag of slots only this frame can see. Each
//! slot becomes a local and the allocation disappears: a field read is a
//! local read, a field write a local write.
//!
//! The unit is a *group*: a root local whose every definition builds an
//! instance of one class, plus the locals that only ever alias it. An alias
//! is a local with a single definition, a copy of the root or of another
//! alias, that provably names the root's current instance wherever it is
//! used. A group is replaced when every mention of its members is one of:
//!
//! - a definition of the root by a class aggregate,
//! - the copy that defines an alias,
//! - a field projection `member.f`, read or written, alone or as the base of
//!   a longer place (`member.f.g`, `member.f[i]`),
//! - a discarding `drop(member)`.
//!
//! Any other mention hands the reference to something this pass does not
//! model (a call, another object, the return slot, a type test, a string
//! join, a closure cell, the unwinder), and the group is left alone.
//!
//! Field locals are written exactly where the instance's slots were, so a
//! trap between two writes leaves a handler the same values either way. A
//! rebuilt root assigns every field local at the point the allocation was
//! stored, after all of its initializers have been evaluated.

use std::collections::VecDeque;

use baml_base::SourceRoot;
use baml_compiler2_hir_ty::{extern_loc::ClassRef, layout};
use baml_type::Literal;

use crate::{
    AggregateKind, BlockId, Constant, FunctionOwner, Local, LocalDecl, MirFunctionBody,
    MirFunctionId, Operand, Place, RuntimeTy, Rvalue, Statement, StatementKind, Terminator, escape,
};

/// What the pass reads outside the body: class layouts and finalizers, as the
/// package owning the body sees them.
pub(super) struct Scope<'a> {
    pub(super) db: &'a dyn crate::Db,
    pub(super) root: SourceRoot,
}

impl<'a> Scope<'a> {
    /// The scope of `function`'s body: a lambda's is its declaring item's.
    pub(super) fn of(db: &'a dyn crate::Db, function: &MirFunctionId<'_>) -> Self {
        fn root(db: &dyn crate::Db, function: &MirFunctionId<'_>) -> SourceRoot {
            match function {
                MirFunctionId::Declared(function) => function.file(db).source_root(db),
                MirFunctionId::Synthetic { parent, .. } => match &**parent {
                    FunctionOwner::Function(function) => function.file(db).source_root(db),
                    FunctionOwner::Let(binding) => binding.file(db).source_root(db),
                    FunctionOwner::Synthetic(function) => root(db, function),
                },
            }
        }
        Self {
            db,
            root: root(db, function),
        }
    }
}

/// Replace every non-escaping instance in `body` with one local per field.
pub(super) fn run(scope: &Scope<'_>, body: &mut MirFunctionBody<'_>, arity: usize) {
    // Replacing an outer instance turns the store of an inner one into a copy
    // between locals, which makes the inner one a candidate. Each round
    // removes at least one aggregate, so this ends.
    while replace_once(scope, body, arity) {}
}

fn replace_once(scope: &Scope<'_>, body: &mut MirFunctionBody<'_>, arity: usize) -> bool {
    let Some(mut groups) = Groups::find(scope, body, arity) else {
        return false;
    };
    groups.find_escapes(body);
    if groups.objects.iter().flatten().all(|object| object.escaped) {
        return false;
    }
    groups.rewrite(scope, body);
    true
}

/// An instance the body builds into a root local.
struct Object<'db> {
    class: ClassRef<'db>,
    /// Slot count of `class`.
    fields: usize,
    /// The locals that alias the root.
    aliases: Vec<Local>,
    /// Per slot: whether the body stores to it after construction.
    written: Vec<bool>,
    /// Whether some mention of a member is not a field access.
    escaped: bool,
}

struct Groups<'db> {
    /// Per local: the instance it is a root of.
    objects: Vec<Option<Object<'db>>>,
    /// Per local: the root of the group it belongs to. A root is its own.
    root: Vec<Option<Local>>,
    /// Per alias: the local its defining copy reads.
    source: Vec<Option<Local>>,
}

impl<'db> Groups<'db> {
    /// The groups of `body`, before any mention is checked. `None` when the
    /// body builds no instance into a local of its own.
    fn find(scope: &Scope<'_>, body: &MirFunctionBody<'db>, arity: usize) -> Option<Self> {
        let n = body.locals.len();
        // Whole-local definitions; a store to a projection is not one.
        let mut defs = vec![0usize; n];
        let mut built = vec![0usize; n];
        let mut class: Vec<Option<ClassRef<'db>>> = vec![None; n];
        let mut one_class = vec![true; n];
        let mut copied: Vec<Option<Local>> = vec![None; n];
        for (_, landing) in body.handlers() {
            defs[landing.error_local.0] += 1;
            defs[landing.context_local.0] += 1;
        }
        for block in &body.blocks {
            for statement in &block.statements {
                match &statement.kind {
                    StatementKind::Assign {
                        destination: Place::Local(local),
                        value,
                    } => {
                        defs[local.0] += 1;
                        match value {
                            Rvalue::Aggregate {
                                kind:
                                    AggregateKind::Class {
                                        class: built_as, ..
                                    },
                                ..
                            } => {
                                built[local.0] += 1;
                                one_class[local.0] &= class[local.0]
                                    .replace(*built_as)
                                    .is_none_or(|c| c == *built_as);
                            }
                            Rvalue::Use(
                                Operand::Copy(Place::Local(source))
                                | Operand::Move(Place::Local(source)),
                            ) => copied[local.0] = Some(*source),
                            _ => {}
                        }
                    }
                    StatementKind::FreshCell { local, .. } => defs[local.0] += 1,
                    _ => {}
                }
            }
            match &block.terminator {
                Some(Terminator::NarrowBind { destination, .. }) => defs[destination.0] += 1,
                Some(terminator) => {
                    if let Some(Place::Local(local)) = terminator_destination(terminator) {
                        defs[local.0] += 1;
                    }
                }
                None => {}
            }
        }
        if built.iter().all(|&count| count == 0) {
            return None;
        }

        // The return slot and parameters are visible to the caller, and a
        // captured local's slot holds a cell.
        let eligible = |local: usize| local > arity && !body.locals[local].is_captured;
        let mut objects: Vec<Option<Object<'db>>> = (0..n).map(|_| None).collect();
        for local in 0..n {
            let Some(class) = class[local] else {
                continue;
            };
            if !eligible(local) || built[local] != defs[local] || !one_class[local] {
                continue;
            }
            // The collector runs a finalizer on the instance, so it has to exist.
            if escape::may_finalize(scope.db, scope.root, &class) {
                continue;
            }
            let fields = layout::class_field_count(scope.db, class);
            objects[local] = Some(Object {
                class,
                fields,
                aliases: Vec::new(),
                written: vec![false; fields],
                escaped: false,
            });
        }

        let mut root: Vec<Option<Local>> = (0..n)
            .map(|local| objects[local].is_some().then_some(Local(local)))
            .collect();
        let mut source: Vec<Option<Local>> = vec![None; n];
        let alias = |local: usize| eligible(local) && defs[local] == 1 && copied[local].is_some();
        for local in 0..n {
            if !alias(local) {
                continue;
            }
            // Follow the copies to a root. A chain longer than the body has
            // locals is a cycle, which only dead code can spell.
            let mut at = local;
            let mut steps = 0;
            while alias(at) && steps <= n {
                at = copied[at].map_or(at, |next| next.0);
                steps += 1;
            }
            if objects[at].is_some() && steps <= n {
                root[local] = Some(Local(at));
                source[local] = copied[local];
                if let Some(object) = &mut objects[at] {
                    object.aliases.push(Local(local));
                }
            }
        }
        Some(Self {
            objects,
            root,
            source,
        })
    }

    fn is_alias(&self, local: Local) -> bool {
        self.source[local.0].is_some()
    }

    /// The effect of `statement` on which aliases name their root's current
    /// instance: rebuilding the root strands them, and an alias is as good as
    /// the local its copy read.
    fn transfer(&self, statement: &StatementKind<'_>, valid: &mut [bool]) {
        let StatementKind::Assign {
            destination: Place::Local(local),
            ..
        } = statement
        else {
            return;
        };
        if let Some(object) = &self.objects[local.0] {
            for alias in &object.aliases {
                valid[alias.0] = false;
            }
        } else if let Some(source) = self.source[local.0] {
            valid[local.0] = !self.is_alias(source) || valid[source.0];
        }
    }

    /// Per block, the aliases that name their root's current instance on
    /// entry, on every path there. A forward must-analysis: nothing holds at
    /// the function's entry, and a handler is entered from every point of the
    /// blocks it protects.
    fn valid_aliases(&self, body: &MirFunctionBody<'_>) -> Vec<Option<Vec<bool>>> {
        fn join(
            target: BlockId,
            state: &[bool],
            entries: &mut [Option<Vec<bool>>],
            work: &mut VecDeque<BlockId>,
        ) {
            let changed = match &mut entries[target.0] {
                Some(entry) => {
                    let mut changed = false;
                    for (mine, theirs) in entry.iter_mut().zip(state) {
                        changed |= *mine && !*theirs;
                        *mine &= *theirs;
                    }
                    changed
                }
                entry @ None => {
                    *entry = Some(state.to_vec());
                    true
                }
            };
            if changed {
                work.push_back(target);
            }
        }

        let mut entries: Vec<Option<Vec<bool>>> = vec![None; body.blocks.len()];
        if self.source.iter().all(Option::is_none) {
            return entries;
        }
        let mut work = VecDeque::new();
        join(
            body.entry,
            &vec![false; body.locals.len()],
            &mut entries,
            &mut work,
        );
        while let Some(id) = work.pop_front() {
            let block = &body.blocks[id.0];
            let mut state = entries[id.0].clone().unwrap_or_default();
            if let Some(handler) = block.unwind {
                join(handler, &state, &mut entries, &mut work);
            }
            for statement in &block.statements {
                self.transfer(&statement.kind, &mut state);
                if let Some(handler) = block.unwind {
                    join(handler, &state, &mut entries, &mut work);
                }
            }
            for successor in block.terminator.iter().flat_map(Terminator::successors) {
                join(successor, &state, &mut entries, &mut work);
            }
        }
        entries
    }

    /// Mark every group with a mention that is not a field access.
    fn find_escapes(&mut self, body: &mut MirFunctionBody<'_>) {
        let entries = self.valid_aliases(body);
        // The visitors take the body mutably to serve the rewrite too; this
        // walk changes nothing.
        for block in &mut body.blocks {
            // An unreached block constrains nothing.
            let mut valid = entries[block.id.0]
                .clone()
                .unwrap_or_else(|| vec![true; self.root.len()]);
            for statement in &mut block.statements {
                match &mut statement.kind {
                    StatementKind::Assign {
                        destination: Place::Local(local),
                        value: Rvalue::Aggregate { fields, .. },
                    } if self.objects[local.0].is_some() => {
                        if self.objects[local.0]
                            .as_ref()
                            .is_some_and(|object| object.fields != fields.len())
                        {
                            self.leak(*local);
                        }
                        for field in fields {
                            visit_operand(field, &mut |place, access| {
                                self.mention(place, Some(access), &valid);
                            });
                        }
                    }
                    // The copy that makes an alias. Whether its source still
                    // names the root is carried into the alias by `transfer`.
                    StatementKind::Assign {
                        destination: Place::Local(local),
                        value: Rvalue::Use(_),
                    } if self.is_alias(*local) => {}
                    StatementKind::Drop(Place::Local(local)) if self.root[local.0].is_some() => {}
                    kind => visit_statement(kind, &mut |place, access| {
                        self.mention(place, Some(access), &valid);
                    }),
                }
                self.transfer(&statement.kind, &mut valid);
            }
            if let Some(terminator) = &mut block.terminator {
                visit_terminator(terminator, &mut |place, access| {
                    self.mention(place, Some(access), &valid);
                });
            }
        }
    }

    /// Check one place the body evaluates. `access` is how the whole place is
    /// used, and `None` for the base of a longer place.
    fn mention(&mut self, place: &Place, access: Option<Access>, valid: &[bool]) {
        match place {
            Place::Local(local) => self.leak(*local),
            Place::Capture(_) => {}
            Place::Deref(cell) => {
                if let Some(local) = cell.local() {
                    self.leak(local);
                }
            }
            Place::Field { base, field } => {
                let member = match &**base {
                    Place::Local(local) => self.root[local.0].map(|root| (*local, root)),
                    _ => None,
                };
                let Some((local, root)) = member else {
                    return self.mention(base, None, valid);
                };
                let current = !self.is_alias(local) || valid[local.0];
                let Some(object) = &mut self.objects[root.0] else {
                    return;
                };
                if !current || *field >= object.fields {
                    object.escaped = true;
                } else if access == Some(Access::Write) {
                    object.written[*field] = true;
                }
            }
            Place::Index { base, index, .. } => {
                self.leak(*index);
                self.mention(base, None, valid);
            }
        }
    }

    /// A member is used as a whole value.
    fn leak(&mut self, local: Local) {
        if let Some(root) = self.root[local.0]
            && let Some(object) = &mut self.objects[root.0]
        {
            object.escaped = true;
        }
    }

    /// Give every surviving group its field locals and rewrite its mentions.
    fn rewrite(&self, scope: &Scope<'_>, body: &mut MirFunctionBody<'_>) {
        let n = body.locals.len();
        // Per local: the root of its group, when the group is replaced.
        let replaced: Vec<Option<Local>> = self
            .root
            .iter()
            .map(|root| {
                root.filter(|root| {
                    self.objects[root.0]
                        .as_ref()
                        .is_some_and(|object| !object.escaped)
                })
            })
            .collect();

        // What every construction stores in each slot, by static type.
        let mut initial: Vec<Vec<Initial>> = (0..n)
            .map(|local| match (&self.objects[local], replaced[local]) {
                (Some(object), Some(_)) => vec![Initial::Unseen; object.fields],
                _ => Vec::new(),
            })
            .collect();
        for statement in body.blocks.iter().flat_map(|block| &block.statements) {
            if let StatementKind::Assign {
                destination: Place::Local(local),
                value: Rvalue::Aggregate { fields, .. },
            } = &statement.kind
                && replaced[local.0] == Some(*local)
            {
                for (initial, field) in initial[local.0].iter_mut().zip(fields) {
                    initial.merge(operand_ty(&body.locals, field));
                }
            }
        }

        let mut slots: Vec<Vec<Local>> = vec![Vec::new(); n];
        for root in 0..n {
            let (Some(object), Some(_)) = (&self.objects[root], replaced[root]) else {
                continue;
            };
            let declared = layout::class_fields(scope.db, object.class);
            for (field, initial) in initial[root].iter().enumerate() {
                let ty = match (initial, declared.get(field)) {
                    // Only the constructions store here, so the slot holds
                    // what they hold.
                    (Initial::Known(ty), _) if !object.written[field] => ty.clone(),
                    // A later store is checked against the declared type.
                    (Initial::Known(ty), Some((_, declared)))
                        if matches!(
                            (declared, ty),
                            (baml_type::Ty::Int, RuntimeTy::Int)
                                | (baml_type::Ty::Float, RuntimeTy::Float)
                                | (baml_type::Ty::Bool, RuntimeTy::Bool)
                        ) =>
                    {
                        ty.clone()
                    }
                    _ => RuntimeTy::Unknown,
                };
                slots[root].push(Local(body.locals.len()));
                body.locals.push(LocalDecl {
                    name: None,
                    ty,
                    span: body.locals[root].span,
                    scope_span: None,
                    is_captured: false,
                });
            }
        }

        let slot_of = |place: &mut Place| replace_field(place, &replaced, &slots);
        let MirFunctionBody { blocks, locals, .. } = body;
        for block in blocks {
            let statements = std::mem::take(&mut block.statements);
            block.statements.reserve(statements.len());
            for mut statement in statements {
                let span = statement.span;
                let assign = |local: Local, operand| Statement {
                    kind: StatementKind::Assign {
                        destination: Place::Local(local),
                        value: Rvalue::Use(operand),
                    },
                    span,
                };
                match &mut statement.kind {
                    StatementKind::Assign {
                        destination: Place::Local(local),
                        value: Rvalue::Aggregate { fields, .. },
                    } if replaced[local.0] == Some(*local) => {
                        let slots = &slots[local.0];
                        // The instance came into being with every slot set at
                        // once, after every initializer was read. An
                        // initializer that reads a place can trap, or can read
                        // a slot this construction overwrites, so those are
                        // all read into temporaries before any slot changes.
                        let mut stores = Vec::with_capacity(slots.len());
                        for (slot, mut field) in slots.iter().zip(std::mem::take(fields)) {
                            visit_operand(&mut field, &mut |place, _| slot_of(place));
                            let direct = match &field {
                                Operand::Constant(_) => true,
                                Operand::Copy(Place::Local(read))
                                | Operand::Move(Place::Local(read)) => !slots.contains(read),
                                Operand::Copy(_) | Operand::Move(_) => false,
                            };
                            if !direct {
                                let read = Local(locals.len());
                                locals.push(locals[slot.0].clone());
                                block.statements.push(assign(read, field));
                                field = Operand::Copy(Place::Local(read));
                            }
                            stores.push(assign(*slot, field));
                        }
                        block.statements.extend(stores);
                    }
                    StatementKind::Assign {
                        destination: Place::Local(local),
                        value: Rvalue::Use(_),
                    } if self.is_alias(*local) && replaced[local.0].is_some() => {}
                    StatementKind::Drop(Place::Local(local)) if replaced[local.0].is_some() => {}
                    kind => {
                        visit_statement(kind, &mut |place, _| slot_of(place));
                        block.statements.push(statement);
                    }
                }
            }
            if let Some(terminator) = &mut block.terminator {
                visit_terminator(terminator, &mut |place, _| slot_of(place));
            }
        }
    }
}

/// Rewrite the field projection of a replaced group's member inside `place`
/// to the slot's local.
fn replace_field(place: &mut Place, replaced: &[Option<Local>], slots: &[Vec<Local>]) {
    match place {
        Place::Field { base, field } => {
            if let Place::Local(local) = &**base
                && let Some(root) = replaced[local.0]
            {
                *place = Place::Local(slots[root.0][*field]);
            } else {
                replace_field(base, replaced, slots);
            }
        }
        Place::Index { base, .. } => replace_field(base, replaced, slots),
        Place::Local(_) | Place::Capture(_) | Place::Deref(_) => {}
    }
}

/// The static type every construction gives a slot.
#[derive(Clone)]
enum Initial {
    Unseen,
    Known(RuntimeTy),
    /// The constructions disagree, or one of them has no usable type.
    Mixed,
}

impl Initial {
    fn merge(&mut self, ty: Option<RuntimeTy>) {
        *self = match (&*self, ty) {
            (Initial::Unseen, Some(ty)) => Initial::Known(ty),
            (Initial::Known(known), Some(ty)) if *known == ty => return,
            _ => Initial::Mixed,
        };
    }
}

/// The type of the value `operand` reads, when its local or constant states
/// one. A literal type widens to its base, so `1` and an `int` local agree.
fn operand_ty(locals: &[LocalDecl], operand: &Operand<'_>) -> Option<RuntimeTy> {
    let ty = match operand {
        Operand::Constant(Constant::Int(_)) => RuntimeTy::Int,
        Operand::Constant(Constant::Float(_)) => RuntimeTy::Float,
        Operand::Constant(Constant::Bool(_)) => RuntimeTy::Bool,
        Operand::Constant(Constant::String(_)) => RuntimeTy::String,
        Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => {
            locals[local.0].ty.clone()
        }
        Operand::Constant(_) | Operand::Copy(_) | Operand::Move(_) => return None,
    };
    match ty {
        RuntimeTy::Literal(Literal::Int(_), _) => Some(RuntimeTy::Int),
        RuntimeTy::Literal(Literal::Float(_), _) => Some(RuntimeTy::Float),
        RuntimeTy::Literal(Literal::Bool(_), _) => Some(RuntimeTy::Bool),
        RuntimeTy::Literal(Literal::String(_), _) => Some(RuntimeTy::String),
        // Lowering types some temporaries `null` while they hold an object.
        RuntimeTy::Null => None,
        ty => Some(ty),
    }
}

/// How a place the body names is used.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Access {
    Read,
    Write,
}

/// The place a terminator stores its result in, on its normal edge.
fn terminator_destination<'a>(terminator: &'a Terminator<'_>) -> Option<&'a Place> {
    match terminator {
        Terminator::Call { destination, .. }
        | Terminator::VirtualCall { destination, .. }
        | Terminator::SysOp { destination, .. }
        | Terminator::Await { destination, .. }
        | Terminator::AwaitAny { destination, .. }
        | Terminator::ShortCircuit { destination, .. } => Some(destination),
        Terminator::Spawn { future, .. } => Some(future),
        Terminator::Goto { .. }
        | Terminator::Branch { .. }
        | Terminator::NarrowBind { .. }
        | Terminator::Switch { .. }
        | Terminator::Return
        | Terminator::Unreachable
        | Terminator::Throw { .. }
        | Terminator::Rethrow { .. } => None,
    }
}

fn visit_operand(operand: &mut Operand<'_>, f: &mut impl FnMut(&mut Place, Access)) {
    match operand {
        Operand::Copy(place) | Operand::Move(place) => f(place, Access::Read),
        Operand::Constant(_) => {}
    }
}

/// Every place `value` evaluates. Exhaustive on purpose: a mention this walk
/// misses is an escape the pass does not see.
fn visit_rvalue(value: &mut Rvalue<'_>, f: &mut impl FnMut(&mut Place, Access)) {
    match value {
        Rvalue::Use(operand)
        | Rvalue::UnaryOp { operand, .. }
        | Rvalue::IsType { operand, .. }
        | Rvalue::IsTypeTag { operand, .. }
        | Rvalue::SatisfiesBound { ty: operand, .. }
        | Rvalue::MakeBoundMethod {
            receiver: operand, ..
        }
        | Rvalue::MakeVirtualBoundMethod {
            receiver: operand, ..
        }
        | Rvalue::VirtualFieldAccess {
            receiver: operand, ..
        }
        | Rvalue::MakeGenericFunctionFromValue { value: operand, .. } => visit_operand(operand, f),
        Rvalue::BinaryOp { left, right, .. } => {
            visit_operand(left, f);
            visit_operand(right, f);
        }
        Rvalue::ConcatStrings(operands)
        | Rvalue::Array(_, operands)
        | Rvalue::Aggregate {
            fields: operands, ..
        }
        | Rvalue::MakeClosure {
            captures: operands, ..
        }
        | Rvalue::MakeVirtualFunction {
            type_args: operands,
            ..
        } => {
            for operand in operands {
                visit_operand(operand, f);
            }
        }
        Rvalue::Map(_, _, entries) => {
            for (key, value) in entries {
                visit_operand(key, f);
                visit_operand(value, f);
            }
        }
        Rvalue::Discriminant(place) | Rvalue::TypeTag(place) | Rvalue::Len(place) => {
            f(place, Access::Read);
        }
        Rvalue::TraceHookSettings { .. }
        | Rvalue::Uint8Array(_)
        | Rvalue::MakeGenericFunction { .. }
        | Rvalue::LoadType(_)
        | Rvalue::CurrentPackage => {}
    }
}

fn visit_statement(statement: &mut StatementKind<'_>, f: &mut impl FnMut(&mut Place, Access)) {
    match statement {
        StatementKind::Assign { destination, value } => {
            visit_rvalue(value, f);
            f(destination, Access::Write);
        }
        StatementKind::Drop(place) => f(place, Access::Read),
        StatementKind::FreshCell { local, .. } => f(&mut Place::Local(*local), Access::Write),
        StatementKind::Intrinsic { args, .. } => {
            for arg in args {
                visit_operand(arg, f);
            }
        }
        StatementKind::VirtualFieldStore {
            receiver, value, ..
        } => {
            visit_operand(receiver, f);
            visit_operand(value, f);
        }
        StatementKind::Nop => {}
    }
}

fn visit_terminator(terminator: &mut Terminator<'_>, f: &mut impl FnMut(&mut Place, Access)) {
    match terminator {
        Terminator::Goto { .. } | Terminator::Return | Terminator::Unreachable => {}
        Terminator::Branch {
            condition: operand, ..
        }
        | Terminator::Switch {
            discriminant: operand,
            ..
        }
        | Terminator::Throw { value: operand } => visit_operand(operand, f),
        Terminator::NarrowBind {
            source,
            destination,
            ..
        } => {
            visit_operand(source, f);
            f(&mut Place::Local(*destination), Access::Write);
        }
        Terminator::Call {
            callee,
            args,
            destination,
            ..
        }
        | Terminator::SysOp {
            callee,
            args,
            destination,
            ..
        } => {
            visit_operand(callee, f);
            for arg in args {
                visit_operand(arg, f);
            }
            f(destination, Access::Write);
        }
        Terminator::VirtualCall {
            args, destination, ..
        } => {
            for arg in args {
                visit_operand(arg, f);
            }
            f(destination, Access::Write);
        }
        Terminator::Spawn { plan, future, .. } => {
            visit_operand(plan, f);
            f(future, Access::Write);
        }
        Terminator::Await {
            future,
            destination,
            ..
        } => {
            f(future, Access::Read);
            f(destination, Access::Write);
        }
        Terminator::AwaitAny {
            futures,
            destination,
            ..
        } => {
            visit_operand(futures, f);
            f(destination, Access::Write);
        }
        Terminator::Rethrow { value, context } => {
            visit_operand(value, f);
            visit_operand(context, f);
        }
        Terminator::ShortCircuit {
            operand,
            destination,
            ..
        } => {
            visit_operand(operand, f);
            f(destination, Access::Write);
        }
    }
}
