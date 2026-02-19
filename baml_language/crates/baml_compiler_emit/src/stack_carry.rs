use std::collections::{HashMap, HashSet};

use baml_compiler_mir::{Local, MirFunction, Operand, Place, Rvalue, StatementKind, Terminator};

use crate::{
    analysis::{LocalClassification, LocalDefUse, StatementRef, UseLocation},
    pull_semantics::{
        self, LocalAssignBehavior, LocalPullAction, LocalStoreBehavior, PullSink, StackEffectSink,
    },
};

/// Stack-carry candidate kinds validated by stack simulation before activation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StackCarryKind {
    PhiLike,
    ReturnPhi,
    CallResultImmediate,
}

impl StackCarryKind {
    fn to_classification(self) -> LocalClassification {
        match self {
            Self::PhiLike => LocalClassification::PhiLike,
            Self::ReturnPhi => LocalClassification::ReturnPhi,
            Self::CallResultImmediate => LocalClassification::CallResultImmediate,
        }
    }
}

/// Refine stack-carried classifications (`PhiLike`, `ReturnPhi`,
/// `CallResultImmediate`) by simulating the emitter's stack behavior.
///
/// We first detect structural candidates, then greedily activate only the
/// candidates whose single use is stack-safe in the current classification map.
pub(super) fn refine_stack_carry_classifications(
    mir: &MirFunction,
    def_use: &HashMap<Local, LocalDefUse>,
    candidates: &HashMap<Local, StackCarryKind>,
    classifications: &mut HashMap<Local, LocalClassification>,
) {
    let mut locals: Vec<Local> = candidates.keys().copied().collect();
    // Deterministic greedy order: this is not a fixpoint search.
    // Some valid candidates may be skipped if they depend on earlier activations.
    locals.sort_by_key(|l| l.0);

    for local in locals {
        let kind = candidates[&local];
        let is_safe = is_stack_carry_use_safe(local, kind, mir, classifications, def_use);
        if is_safe {
            classifications.insert(local, kind.to_classification());
        }
    }
}

fn is_stack_carried_local(classification: LocalClassification) -> bool {
    matches!(
        classification,
        LocalClassification::PhiLike
            | LocalClassification::ReturnPhi
            | LocalClassification::CallResultImmediate
    )
}

#[derive(Clone, Copy, Debug)]
struct StackCarrySim {
    /// Number of stack values above the carried local's value. `None` after the carried
    /// value has been consumed post-use.
    depth: Option<usize>,
    /// Whether we have reached the carried local's single use site.
    used: bool,
}

impl StackCarrySim {
    fn new() -> Self {
        Self {
            depth: Some(0),
            used: false,
        }
    }

    fn push(&mut self) {
        if let Some(depth) = self.depth {
            self.depth = Some(depth + 1);
        }
    }

    fn pop_n(&mut self, n: usize) -> bool {
        if n == 0 {
            return true;
        }

        let Some(depth) = self.depth else {
            // Once the carried value has already been consumed post-use, we stop
            // tracking exact stack depth and treat subsequent pops as irrelevant.
            return true;
        };

        if depth >= n {
            self.depth = Some(depth - n);
            true
        } else if self.used {
            // Carried value consumed after its use site - that's fine.
            self.depth = None;
            true
        } else {
            // Carried value consumed before reaching the use site.
            false
        }
    }
}

fn is_stack_carry_use_safe(
    local: Local,
    kind: StackCarryKind,
    mir: &MirFunction,
    classifications: &HashMap<Local, LocalClassification>,
    def_use: &HashMap<Local, LocalDefUse>,
) -> bool {
    // `analysis::is_return_phi` already proves stack safety for this shape by
    // requiring only stack-neutral statements between def and `Return`.
    if kind == StackCarryKind::ReturnPhi {
        return true;
    }

    let du = &def_use[&local];
    if du.uses.len() != 1 {
        return false;
    }

    let Some(use_loc) = resolve_effective_use_location(&du.uses[0], mir, classifications, def_use)
    else {
        return false;
    };
    let mut sim = StackCarrySim::new();
    let mut current_block = match kind {
        StackCarryKind::PhiLike => use_loc.block,
        StackCarryKind::CallResultImmediate => {
            let Some(def) = &du.def else {
                return false;
            };
            let def_block = mir.block(def.block);
            match &def_block.terminator {
                Some(Terminator::Call {
                    destination,
                    target,
                    ..
                }) => {
                    if !matches!(destination, Place::Local(l) if *l == local) {
                        return false;
                    }
                    *target
                }
                Some(Terminator::Await {
                    destination,
                    target,
                    ..
                }) => {
                    if !matches!(destination, Place::Local(l) if *l == local) {
                        return false;
                    }
                    *target
                }
                Some(Terminator::DispatchFuture { future, resume, .. }) => {
                    if !matches!(future, Place::Local(l) if *l == local) {
                        return false;
                    }
                    *resume
                }
                _ => return false,
            }
        }
        StackCarryKind::ReturnPhi => unreachable!("handled above"),
    };

    let mut visited = HashSet::new();
    loop {
        if !visited.insert(current_block) {
            return false;
        }

        let block = mir.block(current_block);

        if current_block == use_loc.block {
            match use_loc.statement_ref {
                StatementRef::Statement(stmt_idx) => {
                    for stmt in &block.statements[..stmt_idx] {
                        if !simulate_statement_stack(
                            &stmt.kind,
                            &mut sim,
                            local,
                            classifications,
                            def_use,
                        ) {
                            return false;
                        }
                    }

                    let Some(stmt) = block.statements.get(stmt_idx) else {
                        return false;
                    };
                    if !simulate_statement_stack(
                        &stmt.kind,
                        &mut sim,
                        local,
                        classifications,
                        def_use,
                    ) {
                        return false;
                    }
                }
                StatementRef::Terminator => {
                    for stmt in &block.statements {
                        if !simulate_statement_stack(
                            &stmt.kind,
                            &mut sim,
                            local,
                            classifications,
                            def_use,
                        ) {
                            return false;
                        }
                    }

                    let Some(term) = block.terminator.as_ref() else {
                        return false;
                    };
                    if !simulate_terminator_stack(term, &mut sim, local, classifications, def_use) {
                        return false;
                    }
                }
            }

            return sim.used;
        }

        // Intermediate blocks on the carried path must be straight-line gotos.
        for stmt in &block.statements {
            if !simulate_statement_stack(&stmt.kind, &mut sim, local, classifications, def_use) {
                return false;
            }
        }

        let Some(term) = block.terminator.as_ref() else {
            return false;
        };
        let Terminator::Goto { target } = term else {
            return false;
        };

        current_block = *target;
    }
}

fn resolve_effective_use_location(
    initial_use: &UseLocation,
    mir: &MirFunction,
    classifications: &HashMap<Local, LocalClassification>,
    def_use: &HashMap<Local, LocalDefUse>,
) -> Option<UseLocation> {
    let mut current = initial_use.clone();
    let mut visited_forwarded_locals = HashSet::new();

    loop {
        let StatementRef::Statement(stmt_idx) = current.statement_ref else {
            return Some(current);
        };

        let block = mir.block(current.block);
        let stmt = block.statements.get(stmt_idx)?;
        let StatementKind::Assign {
            destination: Place::Local(dest_local),
            ..
        } = &stmt.kind
        else {
            return Some(current);
        };

        let dest_class = classifications
            .get(dest_local)
            .copied()
            .unwrap_or(LocalClassification::Real);

        match dest_class {
            // These assignments are skipped and their value is forwarded to uses
            // of the destination local.
            LocalClassification::Virtual | LocalClassification::CopyOf => {
                if !visited_forwarded_locals.insert(*dest_local) {
                    return None;
                }

                let dest_du = def_use.get(dest_local)?;
                if dest_du.uses.len() != 1 {
                    return None;
                }

                current = dest_du.uses[0].clone();
            }
            LocalClassification::Dead => return None,
            _ => return Some(current),
        }
    }
}

fn simulate_statement_stack(
    kind: &StatementKind,
    sim: &mut StackCarrySim,
    carried_local: Local,
    classifications: &HashMap<Local, LocalClassification>,
    def_use: &HashMap<Local, LocalDefUse>,
) -> bool {
    match kind {
        StatementKind::Assign { destination, value } => match destination {
            Place::Local(dest_local) => {
                let class = classifications
                    .get(dest_local)
                    .copied()
                    .unwrap_or(LocalClassification::Real);

                match pull_semantics::local_assign_behavior(class) {
                    LocalAssignBehavior::Skip => {
                        // Statement skipped entirely in emitter.
                        true
                    }
                    LocalAssignBehavior::EvalNoStore => {
                        // Emit value, skip store.
                        simulate_rvalue_pull_stack(
                            value,
                            sim,
                            carried_local,
                            classifications,
                            def_use,
                        )
                    }
                    LocalAssignBehavior::EvalAndStore => {
                        if !simulate_rvalue_pull_stack(
                            value,
                            sim,
                            carried_local,
                            classifications,
                            def_use,
                        ) {
                            return false;
                        }

                        match pull_semantics::local_store_behavior(class) {
                            LocalStoreBehavior::StoreSlot | LocalStoreBehavior::PopValue => {
                                sim.pop_n(1)
                            }
                            LocalStoreBehavior::KeepOnStack => true,
                        }
                    }
                }
            }
            Place::Field { .. } | Place::Index { .. } => {
                let mut sink = StackCarryPullSink {
                    sim,
                    carried_local,
                    classifications,
                    def_use,
                };
                pull_semantics::walk_projection_store(&mut sink, destination, value).is_ok()
            }
        },
        StatementKind::Drop(place) => {
            let mut sink = StackCarryPullSink {
                sim,
                carried_local,
                classifications,
                def_use,
            };
            pull_semantics::walk_drop_statement(&mut sink, place).is_ok()
        }
        StatementKind::Unwatch(_)
        | StatementKind::NotifyBlock { .. }
        | StatementKind::WatchNotify(_)
        | StatementKind::VizEnter(_)
        | StatementKind::VizExit(_)
        | StatementKind::Nop => true,
        StatementKind::WatchOptions { local, filter } => {
            let mut sink = StackCarryPullSink {
                sim,
                carried_local,
                classifications,
                def_use,
            };
            pull_semantics::walk_watch_options_statement(&mut sink, *local, None, filter).is_ok()
        }
        StatementKind::Assert(operand) => {
            let mut sink = StackCarryPullSink {
                sim,
                carried_local,
                classifications,
                def_use,
            };
            pull_semantics::walk_assert_statement(&mut sink, operand).is_ok()
        }
    }
}

fn simulate_terminator_stack(
    term: &Terminator,
    sim: &mut StackCarrySim,
    carried_local: Local,
    classifications: &HashMap<Local, LocalClassification>,
    def_use: &HashMap<Local, LocalDefUse>,
) -> bool {
    match term {
        Terminator::Goto { .. } | Terminator::Unreachable => true,
        Terminator::Branch { condition, .. } => {
            if !simulate_operand_pull_stack(condition, sim, carried_local, classifications, def_use)
            {
                return false;
            }
            sim.pop_n(1)
        }
        Terminator::Switch { discriminant, .. } => {
            // All switch strategies pull the discriminant first; that's the carried-use point.
            simulate_operand_pull_stack(discriminant, sim, carried_local, classifications, def_use)
        }
        Terminator::Return => {
            let mut sink = StackCarryPullSink {
                sim,
                carried_local,
                classifications,
                def_use,
            };
            if pull_semantics::walk_return_value(&mut sink).is_err() {
                return false;
            }
            sim.pop_n(1)
        }
        Terminator::Call {
            callee,
            args,
            destination,
            ..
        } => {
            let mut sink = StackCarryPullSink {
                sim,
                carried_local,
                classifications,
                def_use,
            };
            if pull_semantics::walk_invoke_operands(&mut sink, callee, args).is_err() {
                return false;
            }

            if !sim.pop_n(args.len() + 1) {
                return false;
            }
            sim.push();
            simulate_store_place_stack(destination, sim, classifications)
        }
        Terminator::DispatchFuture {
            callee,
            args,
            future,
            ..
        } => {
            let mut sink = StackCarryPullSink {
                sim,
                carried_local,
                classifications,
                def_use,
            };
            if pull_semantics::walk_invoke_operands(&mut sink, callee, args).is_err() {
                return false;
            }

            if !sim.pop_n(args.len() + 1) {
                return false;
            }
            sim.push();
            simulate_store_place_stack(future, sim, classifications)
        }
        Terminator::Await {
            future,
            destination,
            ..
        } => {
            let mut sink = StackCarryPullSink {
                sim,
                carried_local,
                classifications,
                def_use,
            };
            if pull_semantics::walk_await_future(&mut sink, future).is_err() {
                return false;
            }
            if !sim.pop_n(1) {
                return false;
            }
            sim.push();
            simulate_store_place_stack(destination, sim, classifications)
        }
    }
}

fn simulate_store_place_stack(
    place: &Place,
    sim: &mut StackCarrySim,
    classifications: &HashMap<Local, LocalClassification>,
) -> bool {
    match place {
        Place::Local(local) => {
            let class = classifications
                .get(local)
                .copied()
                .unwrap_or(LocalClassification::Real);
            match pull_semantics::local_store_behavior(class) {
                LocalStoreBehavior::StoreSlot | LocalStoreBehavior::PopValue => sim.pop_n(1),
                LocalStoreBehavior::KeepOnStack => true,
            }
        }
        Place::Field { .. } | Place::Index { .. } => false,
    }
}

fn simulate_operand_pull_stack(
    operand: &Operand,
    sim: &mut StackCarrySim,
    carried_local: Local,
    classifications: &HashMap<Local, LocalClassification>,
    def_use: &HashMap<Local, LocalDefUse>,
) -> bool {
    let mut sink = StackCarryPullSink {
        sim,
        carried_local,
        classifications,
        def_use,
    };
    pull_semantics::walk_operand_pull(&mut sink, operand).is_ok()
}

fn simulate_rvalue_pull_stack(
    rvalue: &Rvalue,
    sim: &mut StackCarrySim,
    carried_local: Local,
    classifications: &HashMap<Local, LocalClassification>,
    def_use: &HashMap<Local, LocalDefUse>,
) -> bool {
    let mut sink = StackCarryPullSink {
        sim,
        carried_local,
        classifications,
        def_use,
    };
    pull_semantics::walk_rvalue_pull(&mut sink, rvalue).is_ok()
}

struct StackCarryPullSink<'a> {
    sim: &'a mut StackCarrySim,
    carried_local: Local,
    classifications: &'a HashMap<Local, LocalClassification>,
    def_use: &'a HashMap<Local, LocalDefUse>,
}

impl PullSink for StackCarryPullSink<'_> {
    type Error = ();

    fn pull_constant(
        &mut self,
        _constant: &baml_compiler_mir::Constant,
    ) -> Result<(), Self::Error> {
        self.sim.push();
        Ok(())
    }

    fn pull_local(&mut self, local: Local) -> Result<LocalPullAction, Self::Error> {
        if local == self.carried_local {
            if self.sim.depth != Some(0) || self.sim.used {
                return Err(());
            }
            self.sim.used = true;
            return Ok(LocalPullAction::Done);
        }

        let class = self
            .classifications
            .get(&local)
            .copied()
            .unwrap_or(LocalClassification::Real);

        match class {
            LocalClassification::Virtual => {
                let def = self
                    .def_use
                    .get(&local)
                    .and_then(|du| du.def.as_ref())
                    .ok_or(())?;
                Ok(LocalPullAction::Inline(def.rvalue.clone()))
            }
            // Another stack-carried local in this context makes single-local
            // simulation ambiguous; reject to keep the optimization sound.
            other if is_stack_carried_local(other) => Err(()),
            _ => {
                self.sim.push();
                Ok(LocalPullAction::Done)
            }
        }
    }

    fn load_field(&mut self, _field: usize) -> Result<(), Self::Error> {
        if !self.sim.pop_n(1) {
            return Err(());
        }
        self.sim.push();
        Ok(())
    }

    fn load_index(&mut self, _kind: baml_compiler_mir::IndexKind) -> Result<(), Self::Error> {
        if !self.sim.pop_n(2) {
            return Err(());
        }
        self.sim.push();
        Ok(())
    }

    fn binary_op(&mut self, _op: baml_compiler_mir::BinOp) -> Result<(), Self::Error> {
        if !self.sim.pop_n(2) {
            return Err(());
        }
        self.sim.push();
        Ok(())
    }

    fn unary_op(&mut self, _op: baml_compiler_mir::UnaryOp) -> Result<(), Self::Error> {
        if !self.sim.pop_n(1) {
            return Err(());
        }
        self.sim.push();
        Ok(())
    }

    fn alloc_array(&mut self, len: usize) -> Result<(), Self::Error> {
        if !self.sim.pop_n(len) {
            return Err(());
        }
        self.sim.push();
        Ok(())
    }

    fn alloc_map(&mut self, len: usize) -> Result<(), Self::Error> {
        if !self.sim.pop_n(len * 2) {
            return Err(());
        }
        self.sim.push();
        Ok(())
    }

    fn alloc_class_instance(&mut self, _class_name: &str) -> Result<(), Self::Error> {
        // AllocInstance pushes the object reference.
        self.sim.push();
        Ok(())
    }

    fn copy_top(&mut self, _offset: usize) -> Result<(), Self::Error> {
        // Copy duplicates a stack value without consuming the source.
        self.sim.push();
        Ok(())
    }

    fn store_field(&mut self, _field_idx: usize) -> Result<(), Self::Error> {
        // StoreField consumes object + value; class construction leaves original
        // instance below those two entries.
        if !self.sim.pop_n(2) {
            return Err(());
        }
        Ok(())
    }

    fn alloc_enum_variant(&mut self, _enum_name: &str, _variant: &str) -> Result<(), Self::Error> {
        // Emitter loads variant index constant, then AllocVariant (pop1 push1).
        // Net stack effect from this aggregate shape is +1.
        self.sim.push();
        Ok(())
    }

    fn discriminant(&mut self) -> Result<(), Self::Error> {
        if !self.sim.pop_n(1) {
            return Err(());
        }
        self.sim.push();
        Ok(())
    }

    fn type_tag(&mut self) -> Result<(), Self::Error> {
        if !self.sim.pop_n(1) {
            return Err(());
        }
        self.sim.push();
        Ok(())
    }

    fn len_of_place(&mut self, place: &Place) -> Result<(), Self::Error> {
        // Emitter lowers Len as: LoadGlobal(length), <place>, Call(1).
        self.sim.push(); // LoadGlobal pushes callee.
        pull_semantics::walk_place_pull(self, place)?;
        if !self.sim.pop_n(2) {
            return Err(());
        }
        self.sim.push();
        Ok(())
    }

    fn is_type(&mut self, _ty: &baml_type::Ty) -> Result<(), Self::Error> {
        // Emitter consumes operand and pushes boolean result.
        if !self.sim.pop_n(1) {
            return Err(());
        }
        self.sim.push();
        Ok(())
    }
}

impl StackEffectSink for StackCarryPullSink<'_> {
    fn store_field_value(&mut self, _field: usize) -> Result<(), Self::Error> {
        if !self.sim.pop_n(2) {
            return Err(());
        }
        Ok(())
    }

    fn store_index_value(
        &mut self,
        _kind: baml_compiler_mir::IndexKind,
    ) -> Result<(), Self::Error> {
        if !self.sim.pop_n(3) {
            return Err(());
        }
        Ok(())
    }

    fn pop_values(&mut self, n: usize) -> Result<(), Self::Error> {
        if !self.sim.pop_n(n) {
            return Err(());
        }
        Ok(())
    }

    fn push_watch_channel(
        &mut self,
        _local: Local,
        _channel_name: Option<&str>,
    ) -> Result<(), Self::Error> {
        self.sim.push();
        Ok(())
    }

    fn watch_local(&mut self, _local: Local) -> Result<(), Self::Error> {
        if !self.sim.pop_n(2) {
            return Err(());
        }
        Ok(())
    }

    fn assert_top(&mut self) -> Result<(), Self::Error> {
        if !self.sim.pop_n(1) {
            return Err(());
        }
        Ok(())
    }
}
