use std::collections::{HashMap, HashSet};

use baml_compiler_mir::{
    AggregateKind, Local, MirFunction, Operand, Place, Rvalue, StatementKind, Terminator,
};

use super::{LocalClassification, LocalDefUse, StatementRef, UseLocation};

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
    // ReturnPhi already validates stack-safety via stack-neutral statement checks.
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

                match class {
                    LocalClassification::Virtual
                    | LocalClassification::CopyOf
                    | LocalClassification::Dead => {
                        // Statement skipped entirely in emitter.
                        true
                    }
                    LocalClassification::PhiLike | LocalClassification::ReturnPhi => {
                        // Emit value, skip store.
                        simulate_rvalue_pull_stack(
                            value,
                            sim,
                            carried_local,
                            classifications,
                            def_use,
                        )
                    }
                    LocalClassification::Parameter
                    | LocalClassification::Real
                    | LocalClassification::CallResultImmediate => {
                        if !simulate_rvalue_pull_stack(
                            value,
                            sim,
                            carried_local,
                            classifications,
                            def_use,
                        ) {
                            return false;
                        }

                        // Assignment to CallResultImmediate local keeps value on stack.
                        if !matches!(class, LocalClassification::CallResultImmediate) {
                            sim.pop_n(1)
                        } else {
                            true
                        }
                    }
                }
            }
            Place::Field { base, .. } => {
                if !simulate_place_pull_stack(base, sim, carried_local, classifications, def_use) {
                    return false;
                }
                if !simulate_rvalue_pull_stack(value, sim, carried_local, classifications, def_use)
                {
                    return false;
                }
                sim.pop_n(2)
            }
            Place::Index { base, index, .. } => {
                if !simulate_place_pull_stack(base, sim, carried_local, classifications, def_use) {
                    return false;
                }
                if !simulate_place_pull_stack(
                    &Place::Local(*index),
                    sim,
                    carried_local,
                    classifications,
                    def_use,
                ) {
                    return false;
                }
                if !simulate_rvalue_pull_stack(value, sim, carried_local, classifications, def_use)
                {
                    return false;
                }
                sim.pop_n(3)
            }
        },
        StatementKind::Drop(place) => {
            if !simulate_place_pull_stack(place, sim, carried_local, classifications, def_use) {
                return false;
            }
            sim.pop_n(1)
        }
        StatementKind::Unwatch(_)
        | StatementKind::NotifyBlock { .. }
        | StatementKind::WatchNotify(_)
        | StatementKind::VizEnter(_)
        | StatementKind::VizExit(_)
        | StatementKind::Nop => true,
        StatementKind::WatchOptions { filter, .. } => {
            // Emit channel const, filter operand, then Watch pops both.
            sim.push();
            if !simulate_operand_pull_stack(filter, sim, carried_local, classifications, def_use) {
                return false;
            }
            sim.pop_n(2)
        }
        StatementKind::Assert(operand) => {
            if !simulate_operand_pull_stack(operand, sim, carried_local, classifications, def_use) {
                return false;
            }
            sim.pop_n(1)
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
            if !simulate_place_pull_stack(
                &Place::Local(Local(0)),
                sim,
                carried_local,
                classifications,
                def_use,
            ) {
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
            if !simulate_operand_pull_stack(callee, sim, carried_local, classifications, def_use) {
                return false;
            }
            for arg in args {
                if !simulate_operand_pull_stack(arg, sim, carried_local, classifications, def_use) {
                    return false;
                }
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
            if !simulate_operand_pull_stack(callee, sim, carried_local, classifications, def_use) {
                return false;
            }
            for arg in args {
                if !simulate_operand_pull_stack(arg, sim, carried_local, classifications, def_use) {
                    return false;
                }
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
            if !simulate_place_pull_stack(future, sim, carried_local, classifications, def_use) {
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
        Place::Local(local) => match classifications
            .get(local)
            .copied()
            .unwrap_or(LocalClassification::Real)
        {
            LocalClassification::Parameter | LocalClassification::Real => sim.pop_n(1),
            LocalClassification::PhiLike
            | LocalClassification::ReturnPhi
            | LocalClassification::CallResultImmediate => true,
            LocalClassification::Virtual
            | LocalClassification::CopyOf
            | LocalClassification::Dead => sim.pop_n(1),
        },
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
    match operand {
        Operand::Constant(_) => {
            sim.push();
            true
        }
        Operand::Copy(place) | Operand::Move(place) => {
            simulate_place_pull_stack(place, sim, carried_local, classifications, def_use)
        }
    }
}

fn simulate_place_pull_stack(
    place: &Place,
    sim: &mut StackCarrySim,
    carried_local: Local,
    classifications: &HashMap<Local, LocalClassification>,
    def_use: &HashMap<Local, LocalDefUse>,
) -> bool {
    match place {
        Place::Local(local) => {
            if *local == carried_local {
                if sim.depth != Some(0) || sim.used {
                    return false;
                }
                sim.used = true;
                return true;
            }

            let class = classifications
                .get(local)
                .copied()
                .unwrap_or(LocalClassification::Real);
            match class {
                LocalClassification::Virtual => {
                    let Some(def) = def_use.get(local).and_then(|du| du.def.as_ref()) else {
                        return false;
                    };
                    simulate_rvalue_pull_stack(
                        &def.rvalue,
                        sim,
                        carried_local,
                        classifications,
                        def_use,
                    )
                }
                // Another stack-carried local in this context makes single-local simulation
                // ambiguous; reject to keep the optimization sound.
                other if is_stack_carried_local(other) => false,
                _ => {
                    sim.push();
                    true
                }
            }
        }
        Place::Field { base, .. } => {
            if !simulate_place_pull_stack(base, sim, carried_local, classifications, def_use) {
                return false;
            }
            if !sim.pop_n(1) {
                return false;
            }
            sim.push();
            true
        }
        Place::Index { base, index, .. } => {
            if !simulate_place_pull_stack(base, sim, carried_local, classifications, def_use) {
                return false;
            }
            if !simulate_place_pull_stack(
                &Place::Local(*index),
                sim,
                carried_local,
                classifications,
                def_use,
            ) {
                return false;
            }
            if !sim.pop_n(2) {
                return false;
            }
            sim.push();
            true
        }
    }
}

fn simulate_rvalue_pull_stack(
    rvalue: &Rvalue,
    sim: &mut StackCarrySim,
    carried_local: Local,
    classifications: &HashMap<Local, LocalClassification>,
    def_use: &HashMap<Local, LocalDefUse>,
) -> bool {
    match rvalue {
        Rvalue::Use(operand) => {
            simulate_operand_pull_stack(operand, sim, carried_local, classifications, def_use)
        }
        Rvalue::BinaryOp { left, right, .. } => {
            if !simulate_operand_pull_stack(left, sim, carried_local, classifications, def_use) {
                return false;
            }
            if !simulate_operand_pull_stack(right, sim, carried_local, classifications, def_use) {
                return false;
            }
            if !sim.pop_n(2) {
                return false;
            }
            sim.push();
            true
        }
        Rvalue::UnaryOp { operand, .. } => {
            if !simulate_operand_pull_stack(operand, sim, carried_local, classifications, def_use) {
                return false;
            }
            if !sim.pop_n(1) {
                return false;
            }
            sim.push();
            true
        }
        Rvalue::Array(elements) => {
            for elem in elements {
                if !simulate_operand_pull_stack(elem, sim, carried_local, classifications, def_use)
                {
                    return false;
                }
            }
            if !sim.pop_n(elements.len()) {
                return false;
            }
            sim.push();
            true
        }
        Rvalue::Map(entries) => {
            for (_key, value) in entries {
                if !simulate_operand_pull_stack(value, sim, carried_local, classifications, def_use)
                {
                    return false;
                }
            }
            for (key, _value) in entries {
                if !simulate_operand_pull_stack(key, sim, carried_local, classifications, def_use) {
                    return false;
                }
            }
            if !sim.pop_n(entries.len() * 2) {
                return false;
            }
            sim.push();
            true
        }
        Rvalue::Aggregate { kind, fields } => match kind {
            AggregateKind::Array => {
                for field in fields {
                    if !simulate_operand_pull_stack(
                        field,
                        sim,
                        carried_local,
                        classifications,
                        def_use,
                    ) {
                        return false;
                    }
                }
                if !sim.pop_n(fields.len()) {
                    return false;
                }
                sim.push();
                true
            }
            AggregateKind::Class(_) => {
                // AllocInstance
                sim.push();
                // For each field: Copy(0), emit field, StoreField
                for field in fields {
                    sim.push();
                    if !simulate_operand_pull_stack(
                        field,
                        sim,
                        carried_local,
                        classifications,
                        def_use,
                    ) {
                        return false;
                    }
                    if !sim.pop_n(2) {
                        return false;
                    }
                }
                true
            }
            AggregateKind::EnumVariant { .. } => {
                // Load variant index constant then AllocVariant (pop1 push1)
                sim.push();
                if !sim.pop_n(1) {
                    return false;
                }
                sim.push();
                true
            }
        },
        Rvalue::Discriminant(place) | Rvalue::TypeTag(place) => {
            if !simulate_place_pull_stack(place, sim, carried_local, classifications, def_use) {
                return false;
            }
            if !sim.pop_n(1) {
                return false;
            }
            sim.push();
            true
        }
        // TODO: mirror Len/IsType emission edge cases once those paths are cleaned up.
        Rvalue::Len(_) | Rvalue::IsType { .. } => false,
    }
}
