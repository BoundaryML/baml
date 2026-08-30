use std::collections::{HashMap, HashSet};

use baml_compiler2_mir::{
    AggregateKind, BinOp, Constant, Local, MirFunctionBody, Operand, Place, Rvalue, StatementKind,
    Terminator,
};
use baml_type::{Literal, RuntimeTy, TyTemplate};

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
    AggregateOperand,
}

impl StackCarryKind {
    fn to_classification(self) -> LocalClassification {
        match self {
            Self::PhiLike => LocalClassification::PhiLike,
            Self::ReturnPhi => LocalClassification::ReturnPhi,
            Self::CallResultImmediate => LocalClassification::CallResultImmediate,
            Self::AggregateOperand => LocalClassification::AggregateOperand,
        }
    }
}

/// Refine stack-carried classifications (`PhiLike`, `ReturnPhi`,
/// `CallResultImmediate`) by simulating the emitter's stack behavior.
///
/// We first detect structural candidates, then greedily activate only the
/// candidates whose single use is stack-safe in the current classification map.
pub(super) fn refine_stack_carry_classifications(
    body: &MirFunctionBody<'_>,
    def_use: &HashMap<Local, LocalDefUse>,
    candidates: &HashMap<Local, StackCarryKind>,
    classifications: &mut HashMap<Local, LocalClassification>,
) {
    let mut locals: Vec<Local> = candidates.keys().copied().collect();
    // Deterministic greedy order. Aggregate operands are ordered by their use
    // position so earlier stacked values are activated before later ones.
    locals.sort_by_key(|l| stack_carry_sort_key(*l, body, def_use, candidates));

    for local in locals {
        let kind = candidates[&local];
        let is_safe = is_stack_carry_use_safe(local, kind, body, classifications, def_use);
        if is_safe {
            classifications.insert(local, kind.to_classification());
        }
    }
}

fn stack_carry_sort_key(
    local: Local,
    body: &MirFunctionBody<'_>,
    def_use: &HashMap<Local, LocalDefUse>,
    candidates: &HashMap<Local, StackCarryKind>,
) -> (usize, usize, usize, usize) {
    if candidates.get(&local) == Some(&StackCarryKind::AggregateOperand)
        && let Some(use_loc) = def_use.get(&local).and_then(|du| du.uses.first())
        && let StatementRef::Statement(stmt_idx) = use_loc.statement_ref
        && let Some(StatementKind::Assign { value, .. }) = body
            .block(use_loc.block)
            .statements
            .get(stmt_idx)
            .map(|stmt| &stmt.kind)
        && let Some(operand_idx) = aggregate_value_operand_index(value, local)
    {
        return (0, use_loc.block.0, stmt_idx, operand_idx);
    }

    (1, local.0, 0, 0)
}

fn is_stack_carried_local(classification: LocalClassification) -> bool {
    matches!(
        classification,
        LocalClassification::PhiLike
            | LocalClassification::ReturnPhi
            | LocalClassification::CallResultImmediate
            | LocalClassification::AggregateOperand
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

    fn use_carried_at_depth(&mut self, expected_depth: usize) -> bool {
        if self.depth == Some(expected_depth) && !self.used {
            self.used = true;
            true
        } else {
            false
        }
    }
}

fn is_stack_carry_use_safe(
    local: Local,
    kind: StackCarryKind,
    body: &MirFunctionBody<'_>,
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

    let Some(use_loc) = resolve_effective_use_location(&du.uses[0], body, classifications, def_use)
    else {
        return false;
    };
    let mut sim = StackCarrySim::new();
    let mut current_block = match kind {
        StackCarryKind::PhiLike => use_loc.block,
        StackCarryKind::CallResultImmediate | StackCarryKind::AggregateOperand => {
            let Some(def) = &du.def else {
                return false;
            };
            let def_block = body.block(def.block);
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
                Some(Terminator::SysOp {
                    destination,
                    target,
                    ..
                }) => {
                    if !matches!(destination, Place::Local(l) if *l == local) {
                        return false;
                    }
                    *target
                }
                // `AwaitAny` intentionally omitted: its result is never stack-
                // carried (it falls through to `return false` here), because
                // the opcode rewinds + re-executes across the engine suspend
                // and a carried result does not survive that. See the matching
                // note in `analysis.rs` (call-result-immediate checks).
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

        let block = body.block(current_block);

        if current_block == use_loc.block {
            match use_loc.statement_ref {
                StatementRef::Statement(stmt_idx) => {
                    for stmt in &block.statements[..stmt_idx] {
                        if !simulate_statement_stack(
                            &stmt.kind,
                            &mut sim,
                            local,
                            body,
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
                        body,
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
                            body,
                            classifications,
                            def_use,
                        ) {
                            return false;
                        }
                    }

                    let Some(term) = block.terminator.as_ref() else {
                        return false;
                    };
                    if !simulate_terminator_stack(
                        term,
                        &mut sim,
                        local,
                        body,
                        classifications,
                        def_use,
                    ) {
                        return false;
                    }
                }
            }

            return sim.used;
        }

        // Intermediate blocks on the carried path must be straight-line. Aggregate
        // operand carry can cross later call-like terminators because their
        // results may become later aggregate operands stacked above this one.
        for stmt in &block.statements {
            if !simulate_statement_stack(
                &stmt.kind,
                &mut sim,
                local,
                body,
                classifications,
                def_use,
            ) {
                return false;
            }
        }

        let Some(term) = block.terminator.as_ref() else {
            return false;
        };

        current_block = match term {
            Terminator::Goto { target } => *target,
            Terminator::Call { target, .. }
            | Terminator::SysOp { target, .. }
            | Terminator::Await { target, .. }
            | Terminator::AwaitAny { target, .. }
                if kind == StackCarryKind::AggregateOperand =>
            {
                if !simulate_terminator_stack(term, &mut sim, local, body, classifications, def_use)
                {
                    return false;
                }
                *target
            }
            _ => return false,
        };
    }
}

fn resolve_effective_use_location(
    initial_use: &UseLocation,
    body: &MirFunctionBody<'_>,
    classifications: &HashMap<Local, LocalClassification>,
    def_use: &HashMap<Local, LocalDefUse>,
) -> Option<UseLocation> {
    let mut current = initial_use.clone();
    let mut visited_forwarded_locals = HashSet::new();

    loop {
        let StatementRef::Statement(stmt_idx) = current.statement_ref else {
            return Some(current);
        };

        let block = body.block(current.block);
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

fn simulate_statement_stack<'db>(
    kind: &StatementKind<'db>,
    sim: &mut StackCarrySim,
    carried_local: Local,
    body: &MirFunctionBody<'db>,
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
                            body,
                            classifications,
                            def_use,
                        )
                    }
                    LocalAssignBehavior::EvalAndStore => {
                        if !simulate_rvalue_pull_stack(
                            value,
                            sim,
                            carried_local,
                            body,
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
            Place::Capture(_) => {
                // StoreCapture: evaluate rvalue (pops 1), no stack-carry interaction.
                if !simulate_rvalue_pull_stack(
                    value,
                    sim,
                    carried_local,
                    body,
                    classifications,
                    def_use,
                ) {
                    return false;
                }
                sim.pop_n(1)
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
        // Receiver, value and the interface type are pushed then all consumed by the
        // opcode. Rather than simulate that, opt out of stack carry across it — the
        // statement is materialized correctly by `emit_statement`.
        StatementKind::VirtualFieldStore { .. } => false,
        StatementKind::Drop(place) => {
            let mut sink = StackCarryPullSink {
                sim,
                carried_local,
                classifications,
                def_use,
            };
            pull_semantics::walk_drop_statement(&mut sink, place).is_ok()
        }
        StatementKind::FreshCell(_) | StatementKind::Intrinsic { .. } | StatementKind::Nop => true,
    }
}

fn simulate_terminator_stack<'db>(
    term: &Terminator<'db>,
    sim: &mut StackCarrySim,
    carried_local: Local,
    _body: &MirFunctionBody<'db>,
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
        Terminator::NarrowBind { source, .. } => {
            if !simulate_operand_pull_stack(source, sim, carried_local, classifications, def_use) {
                return false;
            }
            sim.pop_n(1)
        }
        Terminator::Switch {
            discriminant,
            arms,
            exhaustive,
            ..
        } => {
            // Simulate the discriminant pull once per pull the chosen emission
            // strategy actually emits (`switch_discriminant_pulls` is derived
            // from the same `SwitchStrategy` the emitter dispatches on, so
            // this simulation and the emitters cannot drift apart). The
            // single-pull strategies consume the carried value exactly once —
            // the carried-use point. The if-else chain re-loads the
            // discriminant per comparison: its second simulated pull finds the
            // carried value already consumed (`sim.used`) — including when the
            // carry is reached through a Virtual chain such as
            // `discriminant(call_result)` — and rejects the candidate, because
            // the emitted pulls 2..N would pop unrelated stack slots (a crash
            // when the popped value is type-incompatible, a SILENT wrong arm
            // when it is compatible). The chain's no-comparison forms (no
            // arms; a single exhaustive arm) pull zero times, so the carried
            // value is never consumed and the region-end `sim.used` check
            // rejects — it would be orphaned on the operand stack. A rejected
            // discriminant takes its regular slot, which every strategy
            // re-loads correctly.
            let pulls = crate::emit::switch_discriminant_pulls(arms, *exhaustive);
            for _ in 0..pulls {
                if !simulate_operand_pull_stack(
                    discriminant,
                    sim,
                    carried_local,
                    classifications,
                    def_use,
                ) {
                    return false;
                }
            }
            true
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
            runtime_id,
            destination,
            ..
        } => {
            let runtime_id_slots = usize::from(runtime_id.is_some());
            let direct_call =
                pull_semantics::resolve_constant_function_item(callee, classifications, def_use)
                    .is_some();
            if direct_call {
                let mut sink = StackCarryPullSink {
                    sim,
                    carried_local,
                    classifications,
                    def_use,
                };
                if pull_semantics::walk_call_direct_args(&mut sink, args).is_err() {
                    return false;
                }
                if let Some(runtime_id) = runtime_id
                    && pull_semantics::walk_operand_pull(&mut sink, runtime_id).is_err()
                {
                    return false;
                }
                if !sim.pop_n(args.len() + runtime_id_slots) {
                    return false;
                }
            } else {
                let mut sink = StackCarryPullSink {
                    sim,
                    carried_local,
                    classifications,
                    def_use,
                };
                if pull_semantics::walk_call_indirect_operands(&mut sink, callee, args).is_err() {
                    return false;
                }
                if let Some(runtime_id) = runtime_id
                    && pull_semantics::walk_operand_pull(&mut sink, runtime_id).is_err()
                {
                    return false;
                }
                if !sim.pop_n(args.len() + 1 + runtime_id_slots) {
                    return false;
                }
            }
            sim.push();
            simulate_store_place_stack(destination, sim, classifications)
        }
        Terminator::VirtualCall {
            args,
            runtime_id,
            destination,
            ..
        } => {
            {
                let mut sink = StackCarryPullSink {
                    sim,
                    carried_local,
                    classifications,
                    def_use,
                };
                if pull_semantics::walk_call_direct_args(&mut sink, args).is_err() {
                    return false;
                }
            }
            // After the value args, emit pushes the interface type (LoadType)
            // and the method name (LoadConst); `VirtualCall` then pops the args
            // plus those two operands and pushes the result.
            sim.push();
            sim.push();
            let runtime_id_slots = if let Some(runtime_id) = runtime_id {
                let mut sink = StackCarryPullSink {
                    sim,
                    carried_local,
                    classifications,
                    def_use,
                };
                if pull_semantics::walk_operand_pull(&mut sink, runtime_id).is_err() {
                    return false;
                }
                1
            } else {
                0
            };
            if !sim.pop_n(args.len() + 2 + runtime_id_slots) {
                return false;
            }
            sim.push();
            simulate_store_place_stack(destination, sim, classifications)
        }
        Terminator::SysOp {
            callee,
            args,
            runtime_id,
            destination,
            ..
        } => {
            if pull_semantics::resolve_constant_function_item(callee, classifications, def_use)
                .is_none()
            {
                return false;
            }

            let mut sink = StackCarryPullSink {
                sim,
                carried_local,
                classifications,
                def_use,
            };
            if pull_semantics::walk_call_direct_args(&mut sink, args).is_err() {
                return false;
            }

            let runtime_id_slots = if let Some(runtime_id) = runtime_id {
                if pull_semantics::walk_operand_pull(&mut sink, runtime_id).is_err() {
                    return false;
                }
                1
            } else {
                0
            };
            if !sim.pop_n(args.len() + runtime_id_slots) {
                return false;
            }
            sim.push();
            simulate_store_place_stack(destination, sim, classifications)
        }
        Terminator::Spawn {
            closure,
            name,
            config,
            future,
            ..
        } => {
            let mut sink = StackCarryPullSink {
                sim,
                carried_local,
                classifications,
                def_use,
            };
            if pull_semantics::walk_operand_pull(&mut sink, closure).is_err() {
                return false;
            }
            if pull_semantics::walk_operand_pull(&mut sink, name).is_err() {
                return false;
            }
            // Config operand is pushed last (null when there is no `with`
            // clause). Mirror `emit`: always push three, pop three. The
            // future's `T`/`E` types are pushed after it by `load_type` and
            // popped again by `Spawn`, so like `alloc_array`'s element type
            // they leave the net stack effect unchanged.
            let null_config = Operand::Constant(Constant::Null);
            let config_op = config.as_deref().unwrap_or(&null_config);
            if pull_semantics::walk_operand_pull(&mut sink, config_op).is_err() {
                return false;
            }
            if !sim.pop_n(3) {
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
        Terminator::AwaitAny {
            futures,
            destination,
            ..
        } => {
            let mut sink = StackCarryPullSink {
                sim,
                carried_local,
                classifications,
                def_use,
            };
            // Push the array operand, then AWAIT_ANY pops it (1) and pushes
            // the winning index (1).
            if pull_semantics::walk_operand_pull(&mut sink, futures).is_err() {
                return false;
            }
            if !sim.pop_n(1) {
                return false;
            }
            sim.push();
            simulate_store_place_stack(destination, sim, classifications)
        }
        Terminator::Throw { value } | Terminator::Rethrow { value } => {
            let mut sink = StackCarryPullSink {
                sim,
                carried_local,
                classifications,
                def_use,
            };
            if pull_semantics::walk_operand_pull(&mut sink, value).is_err() {
                return false;
            }
            // THROW consumes the thrown value from the stack when unwinding.
            sim.pop_n(1)
        }
        Terminator::ThrowIfPanic { value, .. } => {
            let mut sink = StackCarryPullSink {
                sim,
                carried_local,
                classifications,
                def_use,
            };
            if pull_semantics::walk_operand_pull(&mut sink, value).is_err() {
                return false;
            }
            // ThrowIfPanic loads the value, checks it, and either throws (consuming it)
            // or continues (consuming it). Either way the stack is clean after.
            sim.pop_n(1)
        }
        Terminator::ShortCircuit { operand, .. } => {
            // ShortCircuit peeks the operand (stays on TOS), then conditionally
            // keeps or pops+evaluates-rhs. If the carried local is the operand,
            // the peek consumes its stack-carry lifecycle — the short-circuit
            // mechanism takes ownership of the value on TOS. If the carried local
            // was consumed by an earlier statement, the operand pull just adds to
            // the stack which is fine at block-end.
            let mut sink = StackCarryPullSink {
                sim,
                carried_local,
                classifications,
                def_use,
            };
            if pull_semantics::walk_operand_pull(&mut sink, operand).is_err() {
                return false;
            }
            true
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
        // Capture stores: pop 1 (same as StoreSlot).
        Place::Capture(_) => sim.pop_n(1),
        Place::Field { .. } | Place::Index { .. } => false,
    }
}

fn simulate_operand_pull_stack(
    operand: &Operand<'_>,
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

fn simulate_rvalue_pull_stack<'db>(
    rvalue: &Rvalue<'db>,
    sim: &mut StackCarrySim,
    carried_local: Local,
    body: &MirFunctionBody<'db>,
    classifications: &HashMap<Local, LocalClassification>,
    def_use: &HashMap<Local, LocalDefUse>,
) -> bool {
    if let Some(result) =
        simulate_aggregate_operand_pull_stack(rvalue, sim, carried_local, classifications, def_use)
    {
        return result;
    }

    // Class aggregates containing field-copy operands are emitted incrementally
    // as `AllocInstance; InitField/InitSpread`, not as the generic
    // stack-consuming aggregate modeled by `walk_rvalue_pull`. A call result
    // carried into that sequence would sit below the newly allocated instance,
    // reversing the `InitField` operands and treating the call result as the
    // destination object. Reject stack carry for this shape.
    if matches!(
        rvalue,
        Rvalue::Aggregate {
            kind: AggregateKind::Class { .. },
            fields,
        } if fields.iter().any(is_class_field_copy_operand)
    ) {
        return false;
    }

    // The emitter pulls binary operands left-to-right. If the right operand is
    // a carried call result, pulling a safe commutative left operand first puts
    // the stack in reversed order (`right, left`). Numeric add/mul tolerate
    // that order, so the call result can still avoid a slot round-trip.
    if let Rvalue::BinaryOp { op, left, right } = rvalue
        && is_operand_local(right, carried_local)
        && !operand_mentions_local(left, carried_local)
        && is_safe_reversed_commutative_binary(body, *op, left, right)
    {
        if !simulate_operand_pull_stack(left, sim, carried_local, classifications, def_use) {
            return false;
        }
        if !sim.use_carried_at_depth(1) {
            return false;
        }
        if !sim.pop_n(2) {
            return false;
        }
        sim.push();
        return true;
    }

    // MakeBoundMethod: pops receiver (1 value), pushes bound_method (1 value).
    // Net stack effect: 0 (receiver consumed, bound_method produced).
    if let Rvalue::MakeBoundMethod { receiver, .. } = rvalue {
        let mut sink = StackCarryPullSink {
            sim,
            carried_local,
            classifications,
            def_use,
        };
        if pull_semantics::walk_operand_pull(&mut sink, receiver).is_err() {
            return false;
        }
        // Pop receiver, push bound_method (net zero: pop then push).
        if !sim.pop_n(1) {
            return false;
        }
        sim.push();
        return true;
    }
    // MakeVirtualBoundMethod has a variable-arity stack effect (receiver + N method
    // type args + interface type + method name). Rather than simulate it, opt out of
    // the stack-carry optimization for it — `walk_rvalue_pull` panics on it, and it is
    // materialized correctly through `emit_rvalue_pull`.
    // `VirtualFieldAccess` joins it: `walk_rvalue_pull` panics on both, and both are
    // materialized correctly through `emit_rvalue_pull`.
    if matches!(
        rvalue,
        Rvalue::MakeVirtualBoundMethod { .. }
            | Rvalue::MakeVirtualFunction { .. }
            | Rvalue::VirtualFieldAccess { .. }
    ) {
        return false;
    }
    let mut sink = StackCarryPullSink {
        sim,
        carried_local,
        classifications,
        def_use,
    };
    pull_semantics::walk_rvalue_pull(&mut sink, rvalue).is_ok()
}

fn simulate_aggregate_operand_pull_stack(
    rvalue: &Rvalue<'_>,
    sim: &mut StackCarrySim,
    carried_local: Local,
    classifications: &HashMap<Local, LocalClassification>,
    def_use: &HashMap<Local, LocalDefUse>,
) -> Option<bool> {
    match rvalue {
        Rvalue::Array(_, elements) => {
            let values = elements.iter().collect::<Vec<_>>();
            Some(simulate_stack_consuming_aggregate(
                AggregateStackShape {
                    value_operands: &values,
                    trailing_operands: &[],
                    total_pops: elements.len(),
                    extra_pushes_before_alloc: 0,
                },
                sim,
                carried_local,
                classifications,
                def_use,
            ))
        }
        Rvalue::Map(_, _, entries) => {
            // Map keys are trailing operands in VM order. A call result carried
            // from the block entry would be below emitted values, so keys are
            // simulated only as normal operands after the value prefix.
            let values = entries
                .iter()
                .map(|(_key, value)| value)
                .collect::<Vec<_>>();
            let keys = entries.iter().map(|(key, _value)| key).collect::<Vec<_>>();
            Some(simulate_stack_consuming_aggregate(
                AggregateStackShape {
                    value_operands: &values,
                    trailing_operands: &keys,
                    total_pops: entries.len() * 2,
                    extra_pushes_before_alloc: 0,
                },
                sim,
                carried_local,
                classifications,
                def_use,
            ))
        }
        Rvalue::Aggregate {
            kind: baml_compiler2_mir::AggregateKind::Array,
            fields,
        } => {
            let values = fields.iter().collect::<Vec<_>>();
            Some(simulate_stack_consuming_aggregate(
                AggregateStackShape {
                    value_operands: &values,
                    trailing_operands: &[],
                    total_pops: fields.len(),
                    extra_pushes_before_alloc: 0,
                },
                sim,
                carried_local,
                classifications,
                def_use,
            ))
        }
        Rvalue::Aggregate {
            kind:
                baml_compiler2_mir::AggregateKind::Class {
                    type_arg_templates, ..
                },
            fields,
        } if !fields.iter().any(is_class_field_copy_operand) => {
            let values = fields.iter().collect::<Vec<_>>();
            Some(simulate_stack_consuming_aggregate(
                AggregateStackShape {
                    value_operands: &values,
                    trailing_operands: &[],
                    total_pops: fields.len() + type_arg_templates.len(),
                    extra_pushes_before_alloc: type_arg_templates.len(),
                },
                sim,
                carried_local,
                classifications,
                def_use,
            ))
        }
        // Class aggregates with field-copy operands use the `init_spread`
        // emitter path, not the field-value init plan.
        Rvalue::Aggregate { .. } => None,
        _ => None,
    }
}

#[derive(Clone, Copy)]
struct AggregateStackShape<'a> {
    value_operands: &'a [&'a Operand<'a>],
    trailing_operands: &'a [&'a Operand<'a>],
    total_pops: usize,
    extra_pushes_before_alloc: usize,
}

fn simulate_stack_consuming_aggregate(
    shape: AggregateStackShape<'_>,
    sim: &mut StackCarrySim,
    carried_local: Local,
    classifications: &HashMap<Local, LocalClassification>,
    def_use: &HashMap<Local, LocalDefUse>,
) -> bool {
    let mut prefix_len = 0;
    let mut carried_pos = None;

    for operand in shape.value_operands {
        let Some(local) = operand_as_local(operand) else {
            break;
        };

        let classification = classifications
            .get(&local)
            .copied()
            .unwrap_or(LocalClassification::Real);
        if local != carried_local && !is_stack_carried_local(classification) {
            break;
        }

        if local == carried_local {
            carried_pos = Some(prefix_len);
        }
        prefix_len += 1;
    }

    let Some(carried_pos) = carried_pos else {
        return false;
    };
    let expected_depth = prefix_len - carried_pos - 1;
    if !sim.use_carried_at_depth(expected_depth) {
        return false;
    }

    for operand in shape.value_operands.iter().skip(prefix_len) {
        if !simulate_operand_pull_stack(operand, sim, carried_local, classifications, def_use) {
            return false;
        }
    }
    for operand in shape.trailing_operands {
        if !simulate_operand_pull_stack(operand, sim, carried_local, classifications, def_use) {
            return false;
        }
    }
    for _ in 0..shape.extra_pushes_before_alloc {
        sim.push();
    }

    if !sim.pop_n(shape.total_pops) {
        return false;
    }
    sim.push();
    true
}

fn aggregate_value_operand_index<'db>(rvalue: &Rvalue<'db>, local: Local) -> Option<usize> {
    let operands: Vec<&Operand<'db>> = match rvalue {
        Rvalue::Array(_, elements) => elements.iter().collect(),
        // See `simulate_aggregate_operand_pull_stack`: only map values are
        // valid stack-carry prefix operands for the current VM stack layout.
        Rvalue::Map(_, _, entries) => entries.iter().map(|(_key, value)| value).collect(),
        Rvalue::Aggregate {
            kind: baml_compiler2_mir::AggregateKind::Array,
            fields,
        } => fields.iter().collect(),
        Rvalue::Aggregate {
            kind: baml_compiler2_mir::AggregateKind::Class { .. },
            fields,
        } if !fields.iter().any(is_class_field_copy_operand) => fields.iter().collect(),
        Rvalue::Aggregate { .. } => return None,
        _ => return None,
    };

    operands
        .iter()
        .position(|operand| operand_as_local(operand) == Some(local))
}

fn operand_as_local(operand: &Operand<'_>) -> Option<Local> {
    match operand {
        Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => Some(*local),
        _ => None,
    }
}

fn is_class_field_copy_operand(operand: &Operand<'_>) -> bool {
    let place = match operand {
        Operand::Copy(place) | Operand::Move(place) => place,
        Operand::Constant(_) => return false,
    };
    matches!(place, Place::Field { .. })
}

fn is_operand_local(operand: &Operand<'_>, local: Local) -> bool {
    matches!(
        operand,
        Operand::Copy(Place::Local(l)) | Operand::Move(Place::Local(l)) if *l == local
    )
}

fn operand_mentions_local(operand: &Operand<'_>, local: Local) -> bool {
    match operand {
        Operand::Copy(place) | Operand::Move(place) => place_mentions_local(place, local),
        Operand::Constant(_) => false,
    }
}

fn place_mentions_local(place: &Place, local: Local) -> bool {
    match place {
        Place::Local(l) => *l == local,
        Place::Field { base, .. } => place_mentions_local(base, local),
        Place::Index { base, index, .. } => *index == local || place_mentions_local(base, local),
        Place::Capture(_) => false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NumericKind {
    Int,
    Float,
}

fn is_safe_reversed_commutative_binary<'db>(
    body: &MirFunctionBody<'db>,
    op: BinOp,
    left: &Operand<'db>,
    right: &Operand<'db>,
) -> bool {
    match op {
        BinOp::Add | BinOp::Mul => {
            let Some(left_kind) = numeric_operand_kind(body, left) else {
                return false;
            };
            let Some(right_kind) = numeric_operand_kind(body, right) else {
                return false;
            };
            left_kind == right_kind
        }
        _ => false,
    }
}

fn numeric_operand_kind<'db>(
    body: &MirFunctionBody<'db>,
    operand: &Operand<'db>,
) -> Option<NumericKind> {
    match operand {
        Operand::Constant(Constant::Int(_)) => Some(NumericKind::Int),
        Operand::Constant(Constant::Float(_)) => Some(NumericKind::Float),
        Operand::Copy(place) | Operand::Move(place) => numeric_place_kind(body, place),
        Operand::Constant(_) => None,
    }
}

fn numeric_place_kind(body: &MirFunctionBody<'_>, place: &Place) -> Option<NumericKind> {
    match place {
        Place::Local(local) => numeric_ty_kind(&body.local(*local).ty),
        Place::Field { .. } | Place::Index { .. } | Place::Capture(_) => None,
    }
}

fn numeric_ty_kind(ty: &RuntimeTy) -> Option<NumericKind> {
    match ty {
        RuntimeTy::Int { .. } | RuntimeTy::Literal(Literal::Int(_), _, _) => Some(NumericKind::Int),
        RuntimeTy::Float { .. } | RuntimeTy::Literal(Literal::Float(_), _, _) => {
            Some(NumericKind::Float)
        }
        _ => None,
    }
}

struct StackCarryPullSink<'a> {
    sim: &'a mut StackCarrySim,
    carried_local: Local,
    classifications: &'a HashMap<Local, LocalClassification>,
    def_use: &'a HashMap<Local, LocalDefUse<'a>>,
}

impl<'a> PullSink<'a> for StackCarryPullSink<'a> {
    type Error = ();

    fn pull_constant(
        &mut self,
        _constant: &baml_compiler2_mir::Constant<'a>,
    ) -> Result<(), Self::Error> {
        self.sim.push();
        Ok(())
    }

    fn pull_local(&mut self, local: Local) -> Result<LocalPullAction<'a>, Self::Error> {
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
                // These are materialized only by `emit_rvalue_pull`, which
                // intercepts them before the shared walker sees them — so
                // inlining one here would hand `walk_rvalue_pull` an rvalue it
                // asserts it never receives. Reject instead, which is also the
                // honest answer: their stack effects are variable-arity and this
                // simulator does not model them.
                if matches!(
                    def.rvalue,
                    Rvalue::MakeBoundMethod { .. }
                        | Rvalue::MakeVirtualBoundMethod { .. }
                        | Rvalue::MakeVirtualFunction { .. }
                        | Rvalue::VirtualFieldAccess { .. }
                ) {
                    return Err(());
                }
                if let Some(ok) = simulate_aggregate_operand_pull_stack(
                    &def.rvalue,
                    self.sim,
                    self.carried_local,
                    self.classifications,
                    self.def_use,
                ) {
                    return if ok {
                        Ok(LocalPullAction::Done)
                    } else {
                        Err(())
                    };
                }
                Ok(LocalPullAction::Inline(Box::new(def.rvalue.clone())))
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

    fn load_field(&mut self, _field: usize, _name: &str) -> Result<(), Self::Error> {
        if !self.sim.pop_n(1) {
            return Err(());
        }
        self.sim.push();
        Ok(())
    }

    fn load_index(&mut self, _kind: baml_compiler2_mir::IndexKind) -> Result<(), Self::Error> {
        if !self.sim.pop_n(2) {
            return Err(());
        }
        self.sim.push();
        Ok(())
    }

    fn binary_op(&mut self, _op: baml_compiler2_mir::BinOp) -> Result<(), Self::Error> {
        if !self.sim.pop_n(2) {
            return Err(());
        }
        self.sim.push();
        Ok(())
    }

    fn unary_op(&mut self, _op: baml_compiler2_mir::UnaryOp) -> Result<(), Self::Error> {
        if !self.sim.pop_n(1) {
            return Err(());
        }
        self.sim.push();
        Ok(())
    }

    fn alloc_array(&mut self, _element_ty: &TyTemplate, len: usize) -> Result<(), Self::Error> {
        // `load_type` pushes the element type and `AllocArray` pops it again, so
        // the net stack effect (pop `len`, push the array) is unchanged.
        if !self.sim.pop_n(len) {
            return Err(());
        }
        self.sim.push();
        Ok(())
    }

    fn alloc_uint8array(&mut self, _bytes: &[u8]) -> Result<(), Self::Error> {
        // LoadConst pushes 1, Call(deep_copy) pops 1 + pushes 1 → net push 1.
        self.sim.push();
        Ok(())
    }

    fn alloc_map(
        &mut self,
        _key_ty: &TyTemplate,
        _value_ty: &TyTemplate,
        len: usize,
    ) -> Result<(), Self::Error> {
        // `load_type` pushes the key/value types and `AllocMap` pops them again,
        // so the net stack effect (pop `2 * len`, push the map) is unchanged.
        if !self.sim.pop_n(len * 2) {
            return Err(());
        }
        self.sim.push();
        Ok(())
    }

    fn alloc_class_instance(
        &mut self,
        _class_name: &str,
        ntypeargs: u16,
    ) -> Result<(), Self::Error> {
        // LoadType instructions for type args were already emitted and pushed.
        // AllocInstance pops ntypeargs type-arg slots and pushes the instance.
        if ntypeargs > 0 {
            if !self.sim.pop_n(ntypeargs as usize) {
                return Err(());
            }
        }
        self.sim.push();
        Ok(())
    }

    fn init_class_instance(
        &mut self,
        _class_name: &str,
        ntypeargs: u16,
        field_count: usize,
    ) -> Result<(), Self::Error> {
        if !self.sim.pop_n(field_count + usize::from(ntypeargs)) {
            return Err(());
        }
        self.sim.push();
        Ok(())
    }

    fn init_field(&mut self, _field_idx: usize, _name: &str) -> Result<(), Self::Error> {
        // InitField pops only the value; the instance stays on the stack.
        if !self.sim.pop_n(1) {
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
        // Emitter lowers Len as: <place>, Call(length, 1).
        pull_semantics::walk_place_pull(self, place)?;
        if !self.sim.pop_n(1) {
            return Err(());
        }
        self.sim.push();
        Ok(())
    }

    fn is_type(&mut self, _ty: &baml_type::TyTemplate) -> Result<(), Self::Error> {
        // Emitter consumes operand and pushes boolean result.
        if !self.sim.pop_n(1) {
            return Err(());
        }
        self.sim.push();
        Ok(())
    }

    fn is_type_tag(&mut self, _tag: i64) -> Result<(), Self::Error> {
        // Same stack shape as `is_type`: consume the operand, push the bool.
        if !self.sim.pop_n(1) {
            return Err(());
        }
        self.sim.push();
        Ok(())
    }

    fn runtime_is_type(&mut self) -> Result<(), Self::Error> {
        if !self.sim.pop_n(2) {
            return Err(());
        }
        self.sim.push();
        Ok(())
    }

    fn load_type(&mut self, _template: &baml_type::TyTemplate) -> Result<(), Self::Error> {
        // LoadType pushes one Object::Type value onto the stack. No operands consumed.
        self.sim.push();
        Ok(())
    }

    fn load_current_package(&mut self, _package: &str) -> Result<(), Self::Error> {
        self.sim.push();
        Ok(())
    }

    fn make_closure(&mut self, lambda_idx: usize, capture_count: usize) -> Result<(), Self::Error> {
        self.make_closure_with_type_args(lambda_idx, capture_count, 0)
    }

    fn make_closure_with_type_args(
        &mut self,
        _lambda_idx: usize,
        capture_count: usize,
        ntypeargs: usize,
    ) -> Result<(), Self::Error> {
        // MakeClosure pops `ntypeargs` type-arg values, `capture_count` capture
        // values, and pushes one closure object.
        let total_pops = capture_count + ntypeargs;
        if !self.sim.pop_n(total_pops) {
            return Err(());
        }
        self.sim.push();
        Ok(())
    }

    fn make_generic_function(
        &mut self,
        _item: &baml_compiler2_mir::ItemRef<'a>,
        ntypeargs: usize,
    ) -> Result<(), Self::Error> {
        // Pops `ntypeargs` type-arg values, pushes one generic-function object.
        if !self.sim.pop_n(ntypeargs) {
            return Err(());
        }
        self.sim.push();
        Ok(())
    }

    fn make_generic_function_from_value(&mut self, ntypeargs: usize) -> Result<(), Self::Error> {
        // Pops the callable value plus `ntypeargs` type-arg values, pushes one
        // specialized closure object.
        if !self.sim.pop_n(ntypeargs + 1) {
            return Err(());
        }
        self.sim.push();
        Ok(())
    }

    fn load_capture(&mut self, _idx: usize) -> Result<(), Self::Error> {
        // LoadCapture pushes one value onto the stack.
        self.sim.push();
        Ok(())
    }

    fn resolve_field_name(&self, _base: &Place, field_idx: usize) -> String {
        format!("{field_idx}")
    }

    fn class_field_name(&self, _class_name: &str, field_idx: usize) -> String {
        format!("{field_idx}")
    }
}

impl<'a> StackEffectSink<'a> for StackCarryPullSink<'a> {
    fn store_field_value(&mut self, _field: usize, _name: &str) -> Result<(), Self::Error> {
        if !self.sim.pop_n(2) {
            return Err(());
        }
        Ok(())
    }

    fn store_index_value(
        &mut self,
        _kind: baml_compiler2_mir::IndexKind,
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

    fn store_capture_value(&mut self, _idx: usize) -> Result<(), Self::Error> {
        // StoreCapture pops one value (the value to store into the capture cell).
        if !self.sim.pop_n(1) {
            return Err(());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use baml_compiler2_mir::{AggregateKind, BasicBlock, LocalDecl, Statement};
    use baml_type::{RealizedTy, TyAttr, TyTemplate};

    use super::*;

    fn int_ty() -> RuntimeTy {
        RuntimeTy::Int {
            attr: TyAttr::default(),
        }
    }

    fn float_ty() -> RuntimeTy {
        RuntimeTy::Float {
            attr: TyAttr::default(),
        }
    }

    fn local_decl(ty: RuntimeTy) -> LocalDecl {
        LocalDecl {
            name: None,
            ty,
            span: None,
            scope_span: None,
            is_captured: false,
        }
    }

    fn body_with_locals(local_tys: Vec<RuntimeTy>) -> MirFunctionBody<'static> {
        MirFunctionBody {
            blocks: vec![BasicBlock {
                id: baml_compiler2_mir::BlockId(0),
                statements: Vec::<Statement<'static>>::new(),
                terminator: Some(Terminator::Return),
                span: None,
                terminator_span: None,
            }],
            entry: baml_compiler2_mir::BlockId(0),
            locals: local_tys.into_iter().map(local_decl).collect(),
            catch_regions: vec![],
        }
    }

    fn stack_carried_classifications(
        locals: impl IntoIterator<Item = Local>,
    ) -> HashMap<Local, LocalClassification> {
        locals
            .into_iter()
            .map(|local| (local, LocalClassification::CallResultImmediate))
            .collect()
    }

    #[test]
    fn aggregate_stack_carry_accepts_array_value_prefix() {
        let carried = Local(1);
        let sibling = Local(2);
        let classifications = stack_carried_classifications([sibling]);
        let mut sim = StackCarrySim {
            depth: Some(1),
            used: false,
        };

        let ok = simulate_aggregate_operand_pull_stack(
            &Rvalue::Array(
                TyTemplate::from(RealizedTy::unknown()),
                vec![
                    Operand::copy_local(carried),
                    Operand::copy_local(sibling),
                    Operand::Constant(Constant::Int(1)),
                ],
            ),
            &mut sim,
            carried,
            &classifications,
            &HashMap::new(),
        );

        assert_eq!(ok, Some(true));
        assert!(sim.used);
    }

    #[test]
    fn aggregate_stack_carry_accepts_mir_array_aggregate_prefix() {
        let carried = Local(2);
        let sibling = Local(1);
        let classifications = stack_carried_classifications([sibling]);
        let mut sim = StackCarrySim {
            depth: Some(0),
            used: false,
        };

        let ok = simulate_aggregate_operand_pull_stack(
            &Rvalue::Aggregate {
                kind: AggregateKind::Array,
                fields: vec![Operand::copy_local(sibling), Operand::copy_local(carried)],
            },
            &mut sim,
            carried,
            &classifications,
            &HashMap::new(),
        );

        assert_eq!(ok, Some(true));
        assert!(sim.used);
    }

    #[test]
    fn aggregate_stack_carry_simulates_map_values_and_trailing_keys() {
        let carried = Local(1);
        let sibling = Local(2);
        let classifications = stack_carried_classifications([sibling]);
        let mut sim = StackCarrySim {
            depth: Some(1),
            used: false,
        };

        let ok = simulate_aggregate_operand_pull_stack(
            &Rvalue::Map(
                TyTemplate::from(RealizedTy::string()),
                TyTemplate::from(RealizedTy::unknown()),
                vec![
                    (
                        Operand::Constant(Constant::String("a".to_string())),
                        Operand::copy_local(carried),
                    ),
                    (
                        Operand::Constant(Constant::String("b".to_string())),
                        Operand::copy_local(sibling),
                    ),
                ],
            ),
            &mut sim,
            carried,
            &classifications,
            &HashMap::new(),
        );

        assert_eq!(ok, Some(true));
        assert!(sim.used);
    }

    #[test]
    fn map_key_is_not_an_aggregate_value_operand() {
        let key = Local(1);
        let value = Local(2);
        let rvalue = Rvalue::Map(
            TyTemplate::from(RealizedTy::string()),
            TyTemplate::from(RealizedTy::unknown()),
            vec![(Operand::copy_local(key), Operand::copy_local(value))],
        );

        assert_eq!(aggregate_value_operand_index(&rvalue, value), Some(0));
        assert_eq!(aggregate_value_operand_index(&rvalue, key), None);
    }

    #[test]
    fn aggregate_stack_carry_rejects_non_stack_carried_prefix() {
        let carried = Local(2);
        let real_prefix = Local(1);
        let mut sim = StackCarrySim {
            depth: Some(0),
            used: false,
        };

        let ok = simulate_aggregate_operand_pull_stack(
            &Rvalue::Array(
                TyTemplate::from(RealizedTy::unknown()),
                vec![
                    Operand::copy_local(real_prefix),
                    Operand::copy_local(carried),
                ],
            ),
            &mut sim,
            carried,
            &HashMap::new(),
            &HashMap::new(),
        );

        assert_eq!(ok, Some(false));
        assert!(!sim.used);
    }

    #[test]
    fn class_aggregate_stack_carry_accounts_for_type_args() {
        let carried = Local(2);
        let sibling = Local(1);
        let classifications = stack_carried_classifications([sibling]);
        let mut sim = StackCarrySim {
            depth: Some(0),
            used: false,
        };

        let ok = simulate_aggregate_operand_pull_stack(
            &Rvalue::Aggregate {
                kind: AggregateKind::Class {
                    name: "Box".to_string(),
                    type_arg_templates: vec![TyTemplate::from(baml_type::RealizedTy::int())],
                },
                fields: vec![Operand::copy_local(sibling), Operand::copy_local(carried)],
            },
            &mut sim,
            carried,
            &classifications,
            &HashMap::new(),
        );

        assert_eq!(ok, Some(true));
        assert!(sim.used);
    }

    #[test]
    fn class_aggregate_with_field_copy_is_not_modeled_as_init_plan() {
        let carried = Local(1);
        let rvalue = Rvalue::Aggregate {
            kind: AggregateKind::Class {
                name: "Box".to_string(),
                type_arg_templates: vec![],
            },
            fields: vec![Operand::Copy(Place::Field {
                base: Box::new(Place::Local(carried)),
                field: 0,
            })],
        };
        let mut sim = StackCarrySim::new();

        assert_eq!(
            simulate_aggregate_operand_pull_stack(
                &rvalue,
                &mut sim,
                carried,
                &HashMap::new(),
                &HashMap::new(),
            ),
            None
        );
    }

    #[test]
    fn class_spread_rejects_call_result_immediate_before_incremental_init() {
        let carried = Local(1);
        let spread_base = Local(2);
        let rvalue = Rvalue::Aggregate {
            kind: AggregateKind::Class {
                name: "GuideHooks".to_string(),
                type_arg_templates: vec![],
            },
            fields: vec![
                Operand::copy_local(carried),
                Operand::Copy(Place::Field {
                    base: Box::new(Place::Local(spread_base)),
                    field: 1,
                }),
            ],
        };
        let body = body_with_locals(vec![int_ty(), int_ty(), int_ty()]);
        let mut sim = StackCarrySim::new();

        assert!(!simulate_rvalue_pull_stack(
            &rvalue,
            &mut sim,
            carried,
            &body,
            &HashMap::new(),
            &HashMap::new(),
        ));
        assert!(!sim.used);
    }

    #[test]
    fn reversed_commutative_binary_requires_matching_numeric_kinds() {
        let int_body = body_with_locals(vec![int_ty(), int_ty()]);
        assert!(is_safe_reversed_commutative_binary(
            &int_body,
            BinOp::Add,
            &Operand::Constant(Constant::Int(1)),
            &Operand::copy_local(Local(1)),
        ));

        let mixed_body = body_with_locals(vec![int_ty(), float_ty()]);
        assert!(!is_safe_reversed_commutative_binary(
            &mixed_body,
            BinOp::Add,
            &Operand::Constant(Constant::Int(1)),
            &Operand::copy_local(Local(1)),
        ));
    }

    #[test]
    fn rvalue_stack_simulation_accepts_reversed_commutative_call_result() {
        let body = body_with_locals(vec![int_ty(), int_ty()]);
        let carried = Local(1);
        let mut sim = StackCarrySim::new();

        let ok = simulate_rvalue_pull_stack(
            &Rvalue::BinaryOp {
                op: BinOp::Add,
                left: Operand::Constant(Constant::Int(1)),
                right: Operand::copy_local(carried),
            },
            &mut sim,
            carried,
            &body,
            &HashMap::new(),
            &HashMap::new(),
        );

        assert!(ok);
        assert!(sim.used);
    }

    #[test]
    fn init_class_instance_stack_effect_pops_fields_and_type_args() {
        let carried = Local(1);
        let mut sim = StackCarrySim {
            depth: Some(4),
            used: false,
        };
        let classifications = HashMap::new();
        let def_use = HashMap::new();
        let mut sink = StackCarryPullSink {
            sim: &mut sim,
            carried_local: carried,
            classifications: &classifications,
            def_use: &def_use,
        };

        sink.init_class_instance("Box", 1, 2).unwrap();

        assert_eq!(sim.depth, Some(2));
    }
}
