//! MIR analysis for stackification.
//!
//! This module provides:
//! - CFG predecessor computation
//! - Dominator tree computation (Cooper-Harvey-Kennedy algorithm)
//! - Def-use information collection
//! - Local classification (Virtual vs Real)
//! - Jump threading (redirect targets for empty goto-only blocks)
//! - Phi-like local detection (locals assigned in all predecessors, used once at join)
//! - Constant propagation (pure constants with single definition inlined at all use sites)
//! - Call result immediate (single-use Call results used at continuation block start)
//! - Copy propagation (locals that are simple copies of parameters/other locals)
//! - Wildcard elimination (unused `_` pattern bindings are eliminated)

use std::collections::{HashMap, HashSet};

pub use baml_compiler2_mir::OptLevel;
use baml_compiler2_mir::{
    BinOp, BlockId, Constant, Local, MirFunctionBody, Operand, Place, Rvalue, StatementKind,
    Terminator, UnaryOp,
};
use baml_type::{Literal, RuntimeTy};

use crate::stack_carry;

// ============================================================================
// Data Structures
// ============================================================================

/// A reference to either a statement or a terminator within a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum StatementRef {
    /// A statement at the given index.
    Statement(usize),
    /// The block's terminator.
    Terminator,
}

/// Where a local is defined.
#[derive(Clone, Debug)]
pub(crate) struct DefLocation {
    pub block: BlockId,
    pub statement_ref: StatementRef,
    /// The rvalue that produces this local's value (for inlining).
    pub rvalue: Rvalue,
}

/// Where a local is used.
#[derive(Clone, Debug)]
pub(crate) struct UseLocation {
    pub block: BlockId,
    pub statement_ref: StatementRef,
}

/// Def-use information for a single local.
#[derive(Clone, Debug)]
pub(crate) struct LocalDefUse {
    /// Definition site (None for parameters, which are defined at entry).
    pub def: Option<DefLocation>,
    /// All use sites.
    pub uses: Vec<UseLocation>,
    /// All definition sites as `(block, statement_ref)` pairs.
    /// Empty for parameters that are never reassigned.
    pub all_defs: Vec<(BlockId, StatementRef)>,
}

/// Classification of a local variable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalClassification {
    /// Function parameter - always real.
    Parameter,
    /// Multi-use or cross-block local - needs stack slot.
    Real,
    /// Single-use temporary that can be inlined.
    Virtual,
    /// Phi-like local: assigned in each predecessor of a join block, used once at join.
    /// At def sites: emit rvalue but NOT store (leave on stack).
    /// At use site: don't emit `LoadVar` (value already on stack from predecessor).
    PhiLike,
    /// Return-phi: _0 is assigned immediately before Return in each defining block.
    /// At def sites: emit rvalue but NOT store (leave on stack).
    /// At Return: don't emit `LoadVar` for _0 (value already on stack).
    ReturnPhi,
    /// Call result immediate: defined by Call/Await/SysOp, used exactly once
    /// immediately in the continuation block.
    /// At def site (after Call): don't emit Store (leave on stack).
    /// At use site: don't emit `LoadVar` (value already on stack from Call).
    CallResultImmediate,
    /// Call/Await/SysOp result carried as part of a map/array aggregate prefix.
    ///
    /// At def site: don't store the result; leave it on the stack.
    /// At aggregate use site: don't emit `LoadVar`; the aggregate consumes the
    /// already-stacked value in operand order.
    AggregateOperand,
    /// Copy of another local: `_X = copy _Y` where _Y is a parameter or simple local.
    /// At def site: don't emit anything (skip the copy entirely).
    /// At use sites: load from the source local instead.
    /// The source local is stored in `AnalysisResult::copy_sources`.
    CopyOf,
    /// Dead local - defined but never used, can be eliminated.
    Dead,
}

/// Dominator tree.
#[derive(Debug)]
pub(crate) struct Dominators {
    /// Immediate dominator of each block (entry has None).
    pub idom: HashMap<BlockId, Option<BlockId>>,
    /// Reverse postorder indices used by `intersect()` during dominator computation.
    /// The `dead_code` lint fires because the field is only read via a free function,
    /// not through a method on `Dominators`.
    #[allow(dead_code)]
    rpo_idx: HashMap<BlockId, usize>,
}

impl Dominators {
    /// Check if `dominator` dominates `block`.
    pub(crate) fn dominates(&self, dominator: BlockId, block: BlockId) -> bool {
        if dominator == block {
            return true;
        }

        let mut current = block;
        while let Some(Some(idom)) = self.idom.get(&current) {
            if *idom == dominator {
                return true;
            }
            current = *idom;
        }

        false
    }
}

/// Complete analysis result for a function.
#[derive(Debug)]
pub(crate) struct AnalysisResult {
    /// Classification for each local.
    pub classifications: HashMap<Local, LocalClassification>,
    /// Def-use information for each local.
    pub def_use: HashMap<Local, LocalDefUse>,
    /// Reverse postorder of blocks (for iteration).
    pub rpo: Vec<BlockId>,
    /// Jump threading: maps empty goto-only blocks to their final target.
    /// Used during emission to skip intermediate jumps.
    pub redirect_targets: HashMap<BlockId, BlockId>,
    /// Copy propagation: maps locals classified as `CopyOf` to their source local.
    /// When emitting a use of local X, if X is in this map, load from the mapped local instead.
    pub copy_sources: HashMap<Local, Local>,
}

// ============================================================================
// Analysis Entry Point
// ============================================================================

impl AnalysisResult {
    /// Analyze a MIR function and produce classification results.
    pub(crate) fn analyze(body: &MirFunctionBody, arity: usize, opt: OptLevel) -> Self {
        // Step 1: Build predecessor map
        let predecessors = build_predecessors(body);

        // Step 2: Compute reverse postorder
        let rpo = compute_rpo(body);

        // Step 3: Compute dominators
        let dominators = compute_dominators(body, &rpo, &predecessors);

        // Step 4: Collect def-use information
        let def_use = collect_def_use(body);

        // Step 5: Conservative jump threading (truly empty goto-only blocks).
        let initial_redirect_targets = build_redirect_targets(body);

        // Step 6: First classification pass.
        let (mut classifications, mut copy_sources) = classify_locals(
            body,
            arity,
            &def_use,
            &dominators,
            &predecessors,
            &initial_redirect_targets,
            opt,
        );

        // Step 7: Enhanced jump threading using classification info.
        // Some blocks have statements that produce no bytecode (Virtual, Dead,
        // CopyOf assignments). These are effectively empty and can be threaded.
        let redirect_targets = build_redirect_targets_with_classifications(body, &classifications);

        // Step 8: Re-run classification once if redirects changed.
        // `ReturnPhi` checks walk through redirects, so this lets classification
        // observe the final threaded CFG without requiring a general fixpoint loop.
        //
        // NOTE: This bounded refinement is sufficient for the current pipeline because
        // redirect construction only depends on `Virtual | Dead | CopyOf`, which are
        // not redirect-sensitive today. If future MIR optimizations introduce feedback
        // where redirect-sensitive classifications can make blocks newly threadable
        // (or iterative transforms like branch folding/DCE rewrite CFG edges between
        // rounds), upgrade this to a true fixed-point convergence loop.
        if redirect_targets != initial_redirect_targets {
            let (reclassified, recopy_sources) = classify_locals(
                body,
                arity,
                &def_use,
                &dominators,
                &predecessors,
                &redirect_targets,
                opt,
            );
            classifications = reclassified;
            copy_sources = recopy_sources;
        }

        Self {
            classifications,
            def_use,
            rpo,
            redirect_targets,
            copy_sources,
        }
    }

    /// Resolve a jump target through the redirect map.
    /// Returns the final target after following any redirect chains.
    pub(crate) fn resolve_jump_target(&self, target: BlockId) -> BlockId {
        self.redirect_targets
            .get(&target)
            .copied()
            .unwrap_or(target)
    }

    /// Resolve a local through copy propagation.
    /// If the local is a copy of another local, returns the source local.
    /// Follows chains: if A copies B and B copies C, resolves A to C.
    pub(crate) fn resolve_copy_source(&self, local: Local) -> Local {
        let mut current = local;
        while let Some(&source) = self.copy_sources.get(&current) {
            current = source;
        }
        current
    }
}

// ============================================================================
// CFG Analysis
// ============================================================================

/// Build predecessor map for all blocks.
fn build_predecessors(body: &MirFunctionBody) -> HashMap<BlockId, Vec<BlockId>> {
    let mut preds: HashMap<BlockId, Vec<BlockId>> = HashMap::new();

    // Initialize with empty vecs
    for block in &body.blocks {
        preds.insert(block.id, Vec::new());
    }

    // Collect predecessor edges from terminators
    for block in &body.blocks {
        if let Some(term) = &block.terminator {
            for succ in term.successors() {
                if let Some(pred_list) = preds.get_mut(&succ) {
                    pred_list.push(block.id);
                }
            }
        }
    }

    preds
}

/// DFS helper for computing postorder.
fn rpo_dfs(
    body: &MirFunctionBody,
    block_id: BlockId,
    visited: &mut HashSet<BlockId>,
    postorder: &mut Vec<BlockId>,
) {
    if visited.contains(&block_id) {
        return;
    }
    visited.insert(block_id);

    let block = body.block(block_id);
    if let Some(term) = &block.terminator {
        for succ in term.successors() {
            rpo_dfs(body, succ, visited, postorder);
        }
    }
    postorder.push(block_id);
}

/// Compute reverse postorder (depth-first, postorder reversed).
fn compute_rpo(body: &MirFunctionBody) -> Vec<BlockId> {
    let mut visited = HashSet::new();
    let mut postorder = Vec::new();

    // Phase 1: DFS from the entry block. Handlers reachable via CFG edges
    // (Call/Await unwind targets) are visited as descendants of their
    // try-body entry blocks. Layout order does not affect exception-table
    // correctness (the table lists each region's protected blocks' exact PC
    // ranges), so this is purely about code locality and readability.
    rpo_dfs(body, body.entry, &mut visited, &mut postorder);

    // Phase 2: Seed handlers NOT reachable from entry (same-frame panics
    // like division-by-zero where there's no Call/Await with an unwind
    // edge) so they are emitted at all; they land after all entry-reachable
    // blocks in the reversed RPO.
    let mut handler_postorder = Vec::new();
    for region in &body.catch_regions {
        rpo_dfs(body, region.handler, &mut visited, &mut handler_postorder);
    }
    handler_postorder.append(&mut postorder);

    handler_postorder.reverse();
    handler_postorder
}

// ============================================================================
// Emission Helpers
// ============================================================================

/// Check if a block is a "dead" unreachable block that may be skipped during
/// emission without changing observable behavior.
///
/// A block is dead if it has no statements and terminates with `Unreachable`.
pub(crate) fn is_dead_unreachable_block(block: &baml_compiler2_mir::BasicBlock) -> bool {
    block.statements.is_empty() && matches!(block.terminator, Some(Terminator::Unreachable))
}

// ============================================================================
// Jump Threading
// ============================================================================

/// Build redirect targets for jump threading.
///
/// Identifies empty blocks that only contain a Goto terminator and maps them
/// to their final destination. This allows emission to skip intermediate jumps.
fn build_redirect_targets(body: &MirFunctionBody) -> HashMap<BlockId, BlockId> {
    // First pass: identify empty goto-only blocks
    let mut goto_targets: HashMap<BlockId, BlockId> = HashMap::new();

    for block in &body.blocks {
        if block.statements.is_empty() {
            if let Some(Terminator::Goto { target }) = &block.terminator {
                goto_targets.insert(block.id, *target);
            }
        }
    }

    // Second pass: resolve chains (A -> B -> C becomes A -> C)
    let mut resolved: HashMap<BlockId, BlockId> = HashMap::new();

    for &block_id in goto_targets.keys() {
        let final_target = resolve_redirect_chain(block_id, &goto_targets);
        // Only add to resolved if there's actually a redirect
        if final_target != block_id {
            resolved.insert(block_id, final_target);
        }
    }

    resolved
}

/// Follow a chain of redirects to find the final target.
fn resolve_redirect_chain(start: BlockId, goto_targets: &HashMap<BlockId, BlockId>) -> BlockId {
    let mut current = start;
    let mut visited = HashSet::new();

    while let Some(&next) = goto_targets.get(&current) {
        // Avoid infinite loops (shouldn't happen in well-formed MIR)
        if !visited.insert(current) {
            break;
        }
        current = next;
    }

    current
}

/// Build redirect targets using local classification info.
///
/// Like [`build_redirect_targets`] but also threads through blocks whose
/// statements all target locals classified as [`LocalClassification::Virtual`],
/// [`LocalClassification::Dead`], or [`LocalClassification::CopyOf`]. These
/// assignments produce no bytecode during emission, making the block
/// effectively empty.
fn build_redirect_targets_with_classifications(
    body: &MirFunctionBody,
    classifications: &HashMap<Local, LocalClassification>,
) -> HashMap<BlockId, BlockId> {
    let mut goto_targets: HashMap<BlockId, BlockId> = HashMap::new();

    for block in &body.blocks {
        if let Some(target) = threadable_goto_target(block, classifications) {
            goto_targets.insert(block.id, target);
        }
    }

    // Resolve chains (A -> B -> C becomes A -> C).
    let mut resolved: HashMap<BlockId, BlockId> = HashMap::new();

    for &block_id in goto_targets.keys() {
        let final_target = resolve_redirect_chain(block_id, &goto_targets);
        if final_target != block_id {
            resolved.insert(block_id, final_target);
        }
    }

    resolved
}

/// Return the goto target if this block is threadable as an effectively-empty
/// redirect source under the given local classifications.
pub(crate) fn threadable_goto_target(
    block: &baml_compiler2_mir::BasicBlock,
    classifications: &HashMap<Local, LocalClassification>,
) -> Option<BlockId> {
    let Some(Terminator::Goto { target }) = &block.terminator else {
        return None;
    };

    let effectively_empty = block.statements.iter().all(|stmt| {
        matches!(
            &stmt.kind,
            StatementKind::Assign {
                destination: Place::Local(local),
                ..
            } if matches!(
                classifications.get(local),
                Some(
                    LocalClassification::Virtual
                    | LocalClassification::Dead
                    | LocalClassification::CopyOf
                )
            )
        )
    });

    effectively_empty.then_some(*target)
}

// ============================================================================
// Dominator Computation (Cooper-Harvey-Kennedy Algorithm)
// ============================================================================

/// Compute dominators using the Cooper-Harvey-Kennedy algorithm.
///
/// This is a simple, efficient iterative algorithm that computes immediate
/// dominators by repeatedly intersecting dominator sets until convergence.
fn compute_dominators(
    body: &MirFunctionBody,
    rpo: &[BlockId],
    preds: &HashMap<BlockId, Vec<BlockId>>,
) -> Dominators {
    // Map BlockId to RPO index for faster lookup
    let rpo_idx: HashMap<BlockId, usize> = rpo.iter().enumerate().map(|(i, &b)| (b, i)).collect();

    let mut idom: HashMap<BlockId, Option<BlockId>> = HashMap::new();

    // Initialize: entry dominates itself (represented as None for "no parent")
    idom.insert(body.entry, None);

    let mut changed = true;
    while changed {
        changed = false;

        // Skip entry (index 0)
        for &block in &rpo[1..] {
            let predecessors = &preds[&block];

            // Find first predecessor with defined idom
            let mut new_idom = None;
            for &p in predecessors {
                if idom.contains_key(&p) {
                    new_idom = Some(p);
                    break;
                }
            }

            // Intersect with remaining predecessors
            if let Some(mut new_idom_val) = new_idom {
                for &p in predecessors {
                    if idom.contains_key(&p) && p != new_idom_val {
                        // Intersect
                        new_idom_val = intersect(&rpo_idx, &idom, p, new_idom_val);
                    }
                }

                let old = idom.get(&block);
                if old != Some(&Some(new_idom_val)) {
                    idom.insert(block, Some(new_idom_val));
                    changed = true;
                }
            }
        }
    }

    Dominators { idom, rpo_idx }
}

/// Intersect two dominator chains to find their common dominator.
fn intersect(
    rpo_idx: &HashMap<BlockId, usize>,
    idom: &HashMap<BlockId, Option<BlockId>>,
    mut b1: BlockId,
    mut b2: BlockId,
) -> BlockId {
    while b1 != b2 {
        while rpo_idx[&b1] > rpo_idx[&b2] {
            b1 = idom[&b1].expect("should have idom");
        }
        while rpo_idx[&b2] > rpo_idx[&b1] {
            b2 = idom[&b2].expect("should have idom");
        }
    }
    b1
}

// ============================================================================
// Def-Use Collection
// ============================================================================

/// Collect def-use information for all locals.
fn collect_def_use(body: &MirFunctionBody) -> HashMap<Local, LocalDefUse> {
    let mut def_use: HashMap<Local, LocalDefUse> = HashMap::new();

    // Initialize for all locals
    for (idx, _) in body.locals.iter().enumerate() {
        let local = Local(idx);
        def_use.insert(
            local,
            LocalDefUse {
                def: None,
                uses: Vec::new(),
                all_defs: Vec::new(),
            },
        );
    }

    // Walk all blocks
    for block in &body.blocks {
        // Walk statements
        for (stmt_idx, stmt) in block.statements.iter().enumerate() {
            let stmt_ref = StatementRef::Statement(stmt_idx);
            match &stmt.kind {
                StatementKind::Assign { destination, value } => {
                    // Record definition
                    if let Place::Local(local) = destination {
                        if let Some(du) = def_use.get_mut(local) {
                            du.def = Some(DefLocation {
                                block: block.id,
                                statement_ref: stmt_ref,
                                rvalue: value.clone(),
                            });
                            du.all_defs.push((block.id, stmt_ref));
                        }
                    }

                    // For field/index stores, the base local (and index local for Index) is also
                    // used. We need to load them to store the value. This ensures they aren't
                    // classified as Virtual.
                    match destination {
                        Place::Field { base, .. } => {
                            collect_uses_in_place(base, block.id, stmt_ref, &mut def_use);
                        }
                        Place::Index { base, index, .. } => {
                            collect_uses_in_place(base, block.id, stmt_ref, &mut def_use);
                            // The index is also used - we need to load it for the Store*Element
                            def_use.get_mut(index).unwrap().uses.push(UseLocation {
                                block: block.id,
                                statement_ref: stmt_ref,
                            });
                        }
                        Place::Local(_) => {}
                        Place::Capture(_) => {
                            // StoreCapture — no local use to record.
                        }
                    }

                    // Record uses in the rvalue
                    collect_uses_in_rvalue(value, block.id, stmt_ref, &mut def_use);
                }
                StatementKind::VirtualFieldStore {
                    receiver, value, ..
                } => {
                    collect_uses_in_operand(receiver, block.id, stmt_ref, &mut def_use);
                    collect_uses_in_operand(value, block.id, stmt_ref, &mut def_use);
                }
                StatementKind::Drop(place) => {
                    collect_uses_in_place(place, block.id, stmt_ref, &mut def_use);
                }
                StatementKind::Intrinsic { args, .. } => {
                    // Intrinsic args are reads — record uses for each operand
                    for arg in args {
                        collect_uses_in_operand(arg, block.id, stmt_ref, &mut def_use);
                    }
                }
                StatementKind::FreshCell(local) => {
                    // FreshCell only has an effect when the local is captured
                    // (it replaces the cell). For non-captured locals it's a no-op,
                    // so don't add a use that would prevent Virtual classification.
                    if body.local(*local).is_captured {
                        def_use.get_mut(local).unwrap().uses.push(UseLocation {
                            block: block.id,
                            statement_ref: stmt_ref,
                        });
                    }
                }
                StatementKind::VizEnter(_) | StatementKind::VizExit(_) => {
                    // VizEnter/VizExit don't use any locals
                }
                StatementKind::Nop => {}
            }
        }

        // Walk terminator
        if let Some(term) = &block.terminator {
            collect_uses_in_terminator(term, block.id, &mut def_use);
        }
    }

    // Unwind error locals are implicitly used by the exception table —
    // the VM writes into these slots when an exception is caught. Without
    // this, the locals may have zero recorded uses and get classified Dead,
    // causing a panic when the emitter tries to allocate a slot for them.
    for (block_id, local) in body.unwind_error_locals() {
        if let Some(du) = def_use.get_mut(&local) {
            du.uses.push(UseLocation {
                block: block_id,
                statement_ref: StatementRef::Terminator,
            });
        }
    }

    // The VM also materializes the caught error's `ErrorContext` into the
    // context (second-binding) slot, and the BEP-042 cause-chain pre-walk reads
    // it from an *enclosing* handler — uses the static walk can't see. Mark it
    // used so it isn't classified Dead and always gets a slot, even when the
    // `ctx` binding looks statically dead.
    for region in &body.catch_regions {
        if let Some(ctx_local) = region.stack_trace_local
            && let Some(du) = def_use.get_mut(&ctx_local)
        {
            du.uses.push(UseLocation {
                block: region.handler,
                statement_ref: StatementRef::Terminator,
            });
        }
    }

    def_use
}

// ---------------------------------------------------------------------------
// Generic local walkers: single source of truth for traversing MIR trees.
// ---------------------------------------------------------------------------

/// Walk all locals referenced by a place expression, calling `f` for each.
fn walk_place_locals(place: &Place, f: &mut impl FnMut(Local)) {
    match place {
        Place::Local(local) => f(*local),
        Place::Capture(_) => {
            // Captures are not locals — nothing to walk.
        }
        Place::Field { base, .. } => walk_place_locals(base, f),
        Place::Index { base, index, .. } => {
            walk_place_locals(base, f);
            f(*index);
        }
    }
}

/// Walk all locals referenced by an operand, calling `f` for each.
fn walk_operand_locals(operand: &Operand, f: &mut impl FnMut(Local)) {
    match operand {
        Operand::Copy(place) | Operand::Move(place) => walk_place_locals(place, f),
        Operand::Constant(_) => {}
    }
}

/// Walk all locals referenced by an rvalue, calling `f` for each.
fn walk_rvalue_locals(rvalue: &Rvalue, f: &mut impl FnMut(Local)) {
    match rvalue {
        Rvalue::Use(operand) => walk_operand_locals(operand, f),
        Rvalue::BinaryOp { left, right, .. } => {
            walk_operand_locals(left, f);
            walk_operand_locals(right, f);
        }
        Rvalue::UnaryOp { operand, .. } => walk_operand_locals(operand, f),
        Rvalue::Array(_, elements) => {
            for elem in elements {
                walk_operand_locals(elem, f);
            }
        }
        Rvalue::Uint8Array(_) => {}
        Rvalue::Map(_, _, entries) => {
            for (key, value) in entries {
                walk_operand_locals(key, f);
                walk_operand_locals(value, f);
            }
        }
        Rvalue::Aggregate { fields, .. } => {
            for field in fields {
                walk_operand_locals(field, f);
            }
        }
        Rvalue::Discriminant(place) | Rvalue::TypeTag(place) | Rvalue::Len(place) => {
            walk_place_locals(place, f);
        }
        Rvalue::IsType { operand, .. } | Rvalue::IsTypeTag { operand, .. } => {
            walk_operand_locals(operand, f);
        }
        Rvalue::RuntimeIsType {
            operand,
            type_value,
        } => {
            walk_operand_locals(operand, f);
            walk_operand_locals(type_value, f);
        }
        Rvalue::MakeClosure { captures, .. } => {
            for cap in captures {
                walk_operand_locals(cap, f);
            }
        }
        Rvalue::MakeBoundMethod { receiver, .. }
        | Rvalue::MakeVirtualBoundMethod { receiver, .. }
        | Rvalue::VirtualFieldAccess { receiver, .. } => {
            walk_operand_locals(receiver, f);
        }
        Rvalue::MakeVirtualFunction { type_args, .. } => {
            for arg in type_args {
                walk_operand_locals(arg, f);
            }
        }
        Rvalue::LoadType(_) | Rvalue::CurrentPackage(_) | Rvalue::MakeGenericFunction { .. } => {
            // No local operands — the templates are compile-time data.
        }
        Rvalue::MakeGenericFunctionFromValue { value, .. } => {
            walk_operand_locals(value, f);
        }
    }
}

/// Record a use of every local referenced by an rvalue.
fn collect_uses_in_rvalue(
    rvalue: &Rvalue,
    block: BlockId,
    stmt_ref: StatementRef,
    def_use: &mut HashMap<Local, LocalDefUse>,
) {
    walk_rvalue_locals(rvalue, &mut |local| {
        if let Some(du) = def_use.get_mut(&local) {
            du.uses.push(UseLocation {
                block,
                statement_ref: stmt_ref,
            });
        }
    });
}

/// Record a use of every local referenced by an operand.
fn collect_uses_in_operand(
    operand: &Operand,
    block: BlockId,
    stmt_ref: StatementRef,
    def_use: &mut HashMap<Local, LocalDefUse>,
) {
    walk_operand_locals(operand, &mut |local| {
        if let Some(du) = def_use.get_mut(&local) {
            du.uses.push(UseLocation {
                block,
                statement_ref: stmt_ref,
            });
        }
    });
}

/// Record a use of every local referenced by a place.
fn collect_uses_in_place(
    place: &Place,
    block: BlockId,
    stmt_ref: StatementRef,
    def_use: &mut HashMap<Local, LocalDefUse>,
) {
    walk_place_locals(place, &mut |local| {
        if let Some(du) = def_use.get_mut(&local) {
            du.uses.push(UseLocation {
                block,
                statement_ref: stmt_ref,
            });
        }
    });
}

/// Collect uses (and defs for Call/Await) in a terminator.
fn collect_uses_in_terminator(
    term: &Terminator,
    block: BlockId,
    def_use: &mut HashMap<Local, LocalDefUse>,
) {
    match term {
        Terminator::Goto { .. } | Terminator::Unreachable => {}
        Terminator::Return => {
            // Return implicitly uses _0 (the return value local)
            let return_local = Local(0);
            if let Some(du) = def_use.get_mut(&return_local) {
                du.uses.push(UseLocation {
                    block,
                    statement_ref: StatementRef::Terminator,
                });
            }
        }
        Terminator::Branch { condition, .. } => {
            collect_uses_in_operand(condition, block, StatementRef::Terminator, def_use);
        }
        Terminator::NarrowBind {
            source,
            destination,
            ..
        } => {
            collect_uses_in_operand(source, block, StatementRef::Terminator, def_use);
            if let Some(du) = def_use.get_mut(destination) {
                du.def = Some(DefLocation {
                    block,
                    statement_ref: StatementRef::Terminator,
                    rvalue: Rvalue::Use(source.clone()),
                });
                du.all_defs.push((block, StatementRef::Terminator));
            }
        }
        Terminator::Switch { discriminant, .. } => {
            collect_uses_in_operand(discriminant, block, StatementRef::Terminator, def_use);
        }
        Terminator::Call {
            callee,
            args,
            runtime_id,
            destination,
            ..
        } => {
            collect_uses_in_operand(callee, block, StatementRef::Terminator, def_use);
            for arg in args {
                collect_uses_in_operand(arg, block, StatementRef::Terminator, def_use);
            }
            if let Some(runtime_id) = runtime_id {
                collect_uses_in_operand(runtime_id, block, StatementRef::Terminator, def_use);
            }
            // Record the def for the destination (where call result is stored)
            if let Place::Local(local) = destination {
                if let Some(du) = def_use.get_mut(local) {
                    // For Call terminators, we use a synthetic Rvalue::Use with a placeholder
                    // The actual value comes from the call, but for classification purposes,
                    // we just need to know there's a def here
                    du.def = Some(DefLocation {
                        block,
                        statement_ref: StatementRef::Terminator,
                        rvalue: Rvalue::Use(Operand::Constant(Constant::Null)),
                    });
                    du.all_defs.push((block, StatementRef::Terminator));
                }
            }
        }
        Terminator::VirtualCall {
            args,
            runtime_id,
            destination,
            ..
        } => {
            // No callee operand — the method is resolved at runtime from `iface`.
            for arg in args {
                collect_uses_in_operand(arg, block, StatementRef::Terminator, def_use);
            }
            if let Some(runtime_id) = runtime_id {
                collect_uses_in_operand(runtime_id, block, StatementRef::Terminator, def_use);
            }
            // Record the def for the destination (where the call result is stored).
            if let Place::Local(local) = destination {
                if let Some(du) = def_use.get_mut(local) {
                    du.def = Some(DefLocation {
                        block,
                        statement_ref: StatementRef::Terminator,
                        rvalue: Rvalue::Use(Operand::Constant(Constant::Null)),
                    });
                    du.all_defs.push((block, StatementRef::Terminator));
                }
            }
        }
        Terminator::SysOp {
            callee,
            args,
            runtime_id,
            destination,
            ..
        } => {
            collect_uses_in_operand(callee, block, StatementRef::Terminator, def_use);
            for arg in args {
                collect_uses_in_operand(arg, block, StatementRef::Terminator, def_use);
            }
            if let Some(runtime_id) = runtime_id {
                collect_uses_in_operand(runtime_id, block, StatementRef::Terminator, def_use);
            }
            // Record the def for the destination place
            if let Place::Local(local) = destination {
                if let Some(du) = def_use.get_mut(local) {
                    du.def = Some(DefLocation {
                        block,
                        statement_ref: StatementRef::Terminator,
                        rvalue: Rvalue::Use(Operand::Constant(Constant::Null)),
                    });
                    du.all_defs.push((block, StatementRef::Terminator));
                }
            }
        }
        Terminator::Spawn {
            closure,
            name,
            config,
            future,
            ..
        } => {
            collect_uses_in_operand(closure, block, StatementRef::Terminator, def_use);
            collect_uses_in_operand(name, block, StatementRef::Terminator, def_use);
            if let Some(config) = config {
                collect_uses_in_operand(config, block, StatementRef::Terminator, def_use);
            }
            if let Place::Local(local) = future {
                if let Some(du) = def_use.get_mut(local) {
                    du.def = Some(DefLocation {
                        block,
                        statement_ref: StatementRef::Terminator,
                        rvalue: Rvalue::Use(Operand::Constant(Constant::Null)),
                    });
                    du.all_defs.push((block, StatementRef::Terminator));
                }
            }
        }
        Terminator::Await {
            future,
            destination,
            ..
        } => {
            collect_uses_in_place(future, block, StatementRef::Terminator, def_use);
            // Record the def for the destination
            if let Place::Local(local) = destination {
                if let Some(du) = def_use.get_mut(local) {
                    du.def = Some(DefLocation {
                        block,
                        statement_ref: StatementRef::Terminator,
                        rvalue: Rvalue::Use(Operand::Constant(Constant::Null)),
                    });
                    du.all_defs.push((block, StatementRef::Terminator));
                }
            }
        }
        Terminator::AwaitAny {
            futures,
            destination,
            ..
        } => {
            collect_uses_in_operand(futures, block, StatementRef::Terminator, def_use);
            // Record the def for the destination (the winning index)
            if let Place::Local(local) = destination {
                if let Some(du) = def_use.get_mut(local) {
                    du.def = Some(DefLocation {
                        block,
                        statement_ref: StatementRef::Terminator,
                        rvalue: Rvalue::Use(Operand::Constant(Constant::Null)),
                    });
                    du.all_defs.push((block, StatementRef::Terminator));
                }
            }
        }
        Terminator::Throw { value }
        | Terminator::Rethrow { value }
        | Terminator::ThrowIfPanic { value, .. } => {
            collect_uses_in_operand(value, block, StatementRef::Terminator, def_use);
        }
        Terminator::ShortCircuit {
            operand,
            destination,
            ..
        } => {
            collect_uses_in_operand(operand, block, StatementRef::Terminator, def_use);
            // Record the def for the destination
            if let Place::Local(local) = destination {
                if let Some(du) = def_use.get_mut(local) {
                    du.def = Some(DefLocation {
                        block,
                        statement_ref: StatementRef::Terminator,
                        rvalue: Rvalue::Use(Operand::Constant(Constant::Null)),
                    });
                    du.all_defs.push((block, StatementRef::Terminator));
                }
            }
        }
    }
}

// ============================================================================
// Local Classification
// ============================================================================

/// Classify each local as Virtual, Real, `PhiLike`, `CopyOf`, or Dead.
///
/// Returns both the classifications and the `copy_sources` map for copy propagation.
fn classify_locals(
    body: &MirFunctionBody,
    arity: usize,
    def_use: &HashMap<Local, LocalDefUse>,
    dominators: &Dominators,
    predecessors: &HashMap<BlockId, Vec<BlockId>>,
    redirect_targets: &HashMap<BlockId, BlockId>,
    opt: OptLevel,
) -> (HashMap<Local, LocalClassification>, HashMap<Local, Local>) {
    let mut classifications = HashMap::new();
    let mut copy_sources: HashMap<Local, Local> = HashMap::new();
    let mut stack_carry_candidates: HashMap<Local, stack_carry::StackCarryKind> = HashMap::new();
    let narrow_bind_destinations: HashSet<Local> = body
        .blocks
        .iter()
        .filter_map(|block| match block.terminator.as_ref() {
            Some(Terminator::NarrowBind { destination, .. }) => Some(*destination),
            _ => None,
        })
        .collect();

    for (idx, _local_decl) in body.locals.iter().enumerate() {
        let local = Local(idx);
        let du = &def_use[&local];

        let local_decl = body.local(local);

        // Check if this is an unused wildcard binding.
        // NOTE: We currently only check for exactly "_". In the future, we may want
        // more robust checking (e.g., any name starting with "_", or type-based analysis
        // to verify the binding truly has no observable side effects). For now, this
        // simple check handles the common pattern-matching wildcard case.
        let is_unused_wildcard = du.uses.is_empty() && local_decl.name.as_deref() == Some("_");

        // User-named locals (name.is_some()) are kept as Real at O0.
        // Compiler temps have name=None and are always eligible for optimization.
        let is_user_local = local_decl.name.is_some();

        let classification = if idx > 0 && idx <= arity {
            // Parameters are always real (they come from the caller)
            LocalClassification::Parameter
        } else if local_decl.is_captured {
            // Captured locals must always be Real - they need a stable stack slot
            // so that the cell-wrapping preamble (MakeCell/LoadDeref/StoreDeref) works.
            // Virtual/CopyOf/PhiLike classification would inline away the slot.
            LocalClassification::Real
        } else if narrow_bind_destinations.contains(&local) {
            LocalClassification::Real
        } else if idx != 0
            && du.uses.is_empty()
            && (local_decl.name.is_none() || is_unused_wildcard)
        {
            // Dead local: either an unused compiler temp, or an unused wildcard binding.
            // Skip _0 which is implicitly used by return.
            LocalClassification::Dead
        } else if idx != 0
            && let Some(source) = get_copy_source(du, arity, def_use)
        {
            if opt == OptLevel::Zero && is_user_local {
                // At O0, keep user-named locals as Real.
                LocalClassification::Real
            } else {
                // Copy propagation: this local is just `_X = copy _Y` where _Y is suitable.
                // We can eliminate _X and use _Y directly at all use sites.
                copy_sources.insert(local, source);
                LocalClassification::CopyOf
            }
        } else if can_be_virtual(du, dominators, body, arity, def_use, predecessors) {
            if opt == OptLevel::Zero && is_user_local {
                LocalClassification::Real
            } else {
                LocalClassification::Virtual
            }
        } else if is_stack_covered_phi(local, du, body, predecessors) {
            // Stack-carry candidate validated in a later stack simulation pass.
            stack_carry_candidates.insert(local, stack_carry::StackCarryKind::PhiLike);
            LocalClassification::Real
        } else if is_return_phi(local, body, def_use, redirect_targets) {
            // Stack-carry candidate validated in a later stack simulation pass.
            stack_carry_candidates.insert(local, stack_carry::StackCarryKind::ReturnPhi);
            LocalClassification::Real
        } else if is_call_result_aggregate_operand(local, du, body, def_use) {
            // Stack-carry candidate validated in a later stack simulation pass.
            stack_carry_candidates.insert(local, stack_carry::StackCarryKind::AggregateOperand);
            LocalClassification::Real
        } else if is_call_result_immediate(local, du, body) {
            // Stack-carry candidate validated in a later stack simulation pass.
            stack_carry_candidates.insert(local, stack_carry::StackCarryKind::CallResultImmediate);
            LocalClassification::Real
        } else {
            LocalClassification::Real
        };

        classifications.insert(local, classification);
    }

    stack_carry::refine_stack_carry_classifications(
        body,
        def_use,
        &stack_carry_candidates,
        &mut classifications,
    );

    (classifications, copy_sources)
}

/// Check if a local is "phi-like": every path into its single use leaves the
/// local's value on top of the operand stack, so the emitter can drop the
/// `StoreVar`/`LoadVar` pair and let the value ride the CFG edge instead.
///
/// This predicate is the *entire* soundness proof for `StackCarryKind::PhiLike`.
/// The stack simulation in [`crate::stack_carry`] starts AT the use block and
/// only validates that block's own statement prefix — it never inspects the
/// local's definitions, nor the use block's predecessors. So every def this
/// function accepts is emitted as a push with no store, and the use pops
/// exactly one value: an uncovered incoming edge leaves the pop consuming an
/// unrelated value, and a definition off the covered paths leaves a push that
/// nothing pops. Inside a loop the latter grows the operand stack every
/// iteration.
///
/// A local qualifies when all of:
///
/// 1. It has exactly one use, in block `U`.
/// 2. `U` is a join — at least two predecessors. Single-predecessor edges are a
///    different shape and deliberately out of scope here.
/// 3. Every predecessor covers the block it flows into, per
///    [`predecessors_cover_block`].
/// 4. Every definition of the local was recorded while proving (3); any other
///    definition is a stray push.
///
/// Checking only that a `ShortCircuit` terminator's `join` equals `U` is not a
/// substitute for (3): `merge_passthrough_blocks` in `baml_compiler2_mir`
/// rewrites a `ShortCircuit`'s `join` when the original join block is an empty
/// passthrough, and can retarget it onto a block that has unrelated incoming
/// edges. `let x = false; if (c) { x = a && b } x` ends up with the `if` join as
/// both the `ShortCircuit` join and the use block, while its other predecessor
/// is the `Branch` false edge, which pushes nothing.
fn is_stack_covered_phi(
    local: Local,
    du: &LocalDefUse,
    body: &MirFunctionBody,
    predecessors: &HashMap<BlockId, Vec<BlockId>>,
) -> bool {
    if du.uses.len() != 1 || du.all_defs.is_empty() {
        return false;
    }

    let use_block = du.uses[0].block;

    if predecessors
        .get(&use_block)
        .is_none_or(|preds| preds.len() < 2)
    {
        return false;
    }

    let mut visited = HashSet::new();
    let mut covered_defs = HashSet::new();
    if !predecessors_cover_block(
        local,
        use_block,
        body,
        predecessors,
        &mut visited,
        &mut covered_defs,
    ) {
        return false;
    }

    // No stray defs: everything that writes the local must be one of the pushes
    // the coverage walk accounted for.
    du.all_defs.iter().all(|def| covered_defs.contains(def))
}

/// Prove that control cannot reach `block` without the local's value on top of
/// the operand stack, recording the definitions that put it there.
///
/// Each predecessor must do one of:
///
/// a. End in `Goto { target: block }` with its last statement assigning the
///    local — under `PhiLike` the emitter emits the rvalue and skips the store,
///    so the value is left on the stack.
/// b. End in `ShortCircuit { destination: local, join: block, .. }` — the
///    `JumpIfFalse` peek leaves the LHS on the stack on the short-circuit edge.
/// c. Be a statement-free block ending in `Goto { target: block }` whose own
///    predecessors all cover it. The intermediate joins of a chain such as
///    `a && b && c` have this shape.
///
/// `visited` rejects back-edges: a block reachable from itself would need a push
/// per trip to stay balanced, which this shape cannot prove.
///
/// A predecessor that defines the local from a call-like terminator and
/// continues at `block` also leaves the result on the stack, but it is not
/// accepted here: `is_call_result_immediate` documents that the carry path is
/// not wired for every call terminator, so proving that edge needs its own
/// argument rather than a fourth arm bolted onto this one. Locals with such a
/// predecessor stay in a slot — correct, one store/load short of optimal.
fn predecessors_cover_block(
    local: Local,
    block: BlockId,
    body: &MirFunctionBody,
    predecessors: &HashMap<BlockId, Vec<BlockId>>,
    visited: &mut HashSet<BlockId>,
    covered_defs: &mut HashSet<(BlockId, StatementRef)>,
) -> bool {
    if !visited.insert(block) {
        return false;
    }

    let Some(preds) = predecessors.get(&block).filter(|preds| !preds.is_empty()) else {
        return false;
    };

    for &pred_id in preds {
        let pred = body.block(pred_id);

        if let Some(Terminator::ShortCircuit {
            destination: Place::Local(destination),
            join,
            ..
        }) = &pred.terminator
            && *destination == local
            && *join == block
        {
            covered_defs.insert((pred_id, StatementRef::Terminator));
            continue;
        }

        let goes_to_block = matches!(
            &pred.terminator,
            Some(Terminator::Goto { target }) if *target == block
        );
        if !goes_to_block {
            return false;
        }

        let Some(last) = pred.statements.last() else {
            // Empty passthrough — push the proof up to its own predecessors.
            if !predecessors_cover_block(local, pred_id, body, predecessors, visited, covered_defs)
            {
                return false;
            }
            continue;
        };

        let assigns_local = matches!(
            &last.kind,
            StatementKind::Assign { destination: Place::Local(l), .. } if *l == local
        );
        if !assigns_local {
            return false;
        }

        covered_defs.insert((pred_id, StatementRef::Statement(pred.statements.len() - 1)));
    }

    true
}

/// Check if a MIR statement is stack-neutral (doesn't push or pop from the eval stack).
///
/// Stack-neutral statements can safely execute while a value meant for return sits on
/// the stack, enabling optimizations like `ReturnPhi` even when there are statements
/// between the assignment to `_0` and the `Return` terminator.
fn is_stack_neutral_statement(kind: &StatementKind) -> bool {
    match kind {
        // These don't touch the stack at all - just update external state
        StatementKind::VizEnter(_) | StatementKind::VizExit(_) => true,
        StatementKind::FreshCell(_) => true,
        // Intrinsics push args then SendEvent consumes them - net neutral
        StatementKind::Intrinsic { .. } => true,
        StatementKind::Nop => true,

        // These modify the stack
        StatementKind::Assign { .. } => false,
        StatementKind::Drop(_) => false,
        // Pushes receiver, value and the interface type, then the opcode pops all
        // three — net neutral, but it touches the stack in between, so a value
        // parked there for `Return` would be buried.
        StatementKind::VirtualFieldStore { .. } => false,
    }
}

/// Check if `_0` (the return place) is a "return-phi" local.
///
/// Return-phi applies when `_0` is assigned before Return in each defining block,
/// with only stack-neutral statements (like `VizExit`) between the assignment
/// and Return. This allows us to:
/// - At def sites: emit rvalue but NOT `StoreVar` (leave value on stack)
/// - At Return: skip `LoadVar` for _0 (value already on stack)
///
/// This eliminates the redundant `StoreVar("_0"); LoadVar("_0"); Return` pattern.
fn is_return_phi(
    local: Local,
    body: &MirFunctionBody,
    def_use: &HashMap<Local, LocalDefUse>,
    redirect_targets: &HashMap<BlockId, BlockId>,
) -> bool {
    // Only applies to _0 (the return place)
    if local.0 != 0 {
        return false;
    }

    // Get all definitions of _0
    let defs = &def_use[&local].all_defs;

    // Must have at least one definition
    if defs.is_empty() {
        return false;
    }

    // Helper: check if a block leads to Return through only stack-neutral statements.
    // Follows Goto chains, ensuring all intermediate blocks have only stack-neutral statements.
    let leads_to_return_safely = |start: BlockId| -> bool {
        let mut current = start;
        let mut visited = HashSet::new();

        loop {
            // Avoid infinite loops
            if !visited.insert(current) {
                return false;
            }

            let block = body.block(current);

            // All statements in this block must be stack-neutral
            if !block
                .statements
                .iter()
                .all(|s| is_stack_neutral_statement(&s.kind))
            {
                return false;
            }

            match &block.terminator {
                Some(Terminator::Return) => return true,
                Some(Terminator::Goto { target }) => {
                    // Follow the redirect chain
                    current = redirect_targets.get(target).copied().unwrap_or(*target);
                }
                _ => return false,
            }
        }
    };

    // Each definition block must:
    // 1. Have the definition followed only by stack-neutral statements (or be a terminator definition)
    // 2. End with Return OR lead to Return through only stack-neutral blocks
    for &(block_id, stmt_ref) in defs {
        let block = body.block(block_id);

        let stmt_idx = match stmt_ref {
            StatementRef::Terminator => {
                // For terminator definitions, check if the continuation leads to return safely
                let continuation = match &block.terminator {
                    Some(Terminator::Call { target, .. }) => Some(*target),
                    Some(Terminator::SysOp { target, .. }) => Some(*target),
                    Some(Terminator::Await { target, .. }) => Some(*target),
                    Some(Terminator::AwaitAny { target, .. }) => Some(*target),
                    _ => None,
                };
                let valid = continuation.is_some_and(leads_to_return_safely);
                if !valid {
                    return false;
                }
                continue;
            }
            StatementRef::Statement(idx) => idx,
        };

        // For regular Assign statements: all statements after the definition must be stack-neutral
        let statements_after_def_are_neutral = block.statements[stmt_idx + 1..]
            .iter()
            .all(|s| is_stack_neutral_statement(&s.kind));
        if !statements_after_def_are_neutral {
            return false;
        }

        // Block must end with Return or lead to Return through stack-neutral blocks
        let valid_terminator = match &block.terminator {
            Some(Terminator::Return) => true,
            Some(Terminator::Goto { target }) => {
                let resolved = redirect_targets.get(target).copied().unwrap_or(*target);
                leads_to_return_safely(resolved)
            }
            _ => false,
        };

        if !valid_terminator {
            return false;
        }
    }

    true
}

/// Check if a local can be classified as Virtual.
fn can_be_virtual(
    du: &LocalDefUse,
    dominators: &Dominators,
    body: &MirFunctionBody,
    arity: usize,
    def_use: &HashMap<Local, LocalDefUse>,
    predecessors: &HashMap<BlockId, Vec<BlockId>>,
) -> bool {
    // Must have exactly one definition
    let Some(def) = &du.def else {
        return false;
    };

    // Definitions in terminators (Call/Await/SysOp) cannot be inlined
    // because the value comes from the operation itself, not from a re-emittable rvalue
    if def.statement_ref == StatementRef::Terminator {
        return false;
    }

    // Pure constants with a SINGLE definition can be inlined even with multiple uses.
    // They have no side effects and always produce the same value.
    // If there are multiple definitions (e.g., from if-else branches), we can't inline
    // because we'd inline the wrong definition for some execution paths.
    let has_single_def = du.all_defs.len() == 1;
    if has_single_def && is_pure_constant(&def.rvalue) {
        // Just need at least one use to not be dead
        return !du.uses.is_empty();
    }

    // `Rvalue::Len` must be materialized eagerly at the binding site.
    // Re-evaluating a virtualized `len` after intervening mutations (e.g.
    // `push`) changes observable semantics for `let` bindings.
    if matches!(def.rvalue, Rvalue::Len(_)) {
        return false;
    }

    // For non-constant rvalues, require exactly one definition site.
    // Virtual emission inlines `du.def` directly; multiple defs would be ambiguous.
    if !has_single_def {
        return false;
    }

    // For non-constant rvalues, must have exactly one use
    if du.uses.len() != 1 {
        return false;
    }

    let use_loc = &du.uses[0];

    // Definition must dominate use
    if !dominators.dominates(def.block, use_loc.block) {
        return false;
    }

    // The def must be a statement (terminator defs were rejected above).
    let StatementRef::Statement(def_idx) = def.statement_ref else {
        unreachable!("terminator defs already rejected");
    };

    // If in same block, use must come after def
    if def.block == use_loc.block {
        match use_loc.statement_ref {
            StatementRef::Terminator => {
                // Terminator always comes after all statements, so this is fine
                // But check for side effects between def and end of block
                if has_side_effects_between(
                    body,
                    def.block,
                    def_idx + 1,
                    body.block(def.block).statements.len(),
                    &def.rvalue,
                    def_use,
                ) {
                    return false;
                }
            }
            StatementRef::Statement(use_idx) => {
                if use_idx <= def_idx {
                    return false;
                }
                // Check for intervening side effects
                if has_side_effects_between(
                    body,
                    def.block,
                    def_idx + 1,
                    use_idx,
                    &def.rvalue,
                    def_use,
                ) {
                    return false;
                }
            }
        }
    } else {
        // Cross-block def-use: the rvalue will be re-evaluated at the use site,
        // so we must ensure no path from def to use modifies any dependency.
        //
        // Reads through projections (field/index) are especially hard to reason about
        // with this local-only analysis because writes to `x.field` don't appear as
        // defs of `x`. Be conservative and avoid cross-block inlining for those.
        if rvalue_has_projection_reads(&def.rvalue) {
            return false;
        }
        // A panicking evaluation is itself observable, and every path from the
        // def block to the use block crosses at least the def block's
        // terminator — a call, whose effects would then run before the panic.
        // The use site can also sit in a different exception region than the
        // def, which changes the handler and can double-run a `defer` body
        // (once inline on the way out, once in the unwind landing pad).
        if rvalue_can_panic(body, &def.rvalue) {
            return false;
        }
        //
        // Rather than walking all intermediate blocks (which requires full path
        // enumeration), we use a sound conservative check: if any local read by
        // the rvalue (including transitive same-block deps) has multiple
        // definitions, it may be modified on some path between def and use, so
        // we refuse to virtualize.
        let reads = collect_transitive_reads(&def.rvalue, def_use, def.block, def_idx);

        for read_local in &reads {
            if let Some(read_du) = def_use.get(read_local) {
                // Parameters have an implicit entry definition not tracked
                // in all_defs, so any explicit def means multiple definitions.
                let is_param = read_local.0 > 0 && read_local.0 <= arity;
                let has_multiple_defs = if is_param {
                    !read_du.all_defs.is_empty()
                } else {
                    read_du.all_defs.len() > 1
                };
                if has_multiple_defs {
                    return false;
                }
            }
        }

        // Preserve the existing protection for values used directly in a loop
        // header.
        let use_preds = predecessors
            .get(&use_loc.block)
            .map_or(&[] as &[_], Vec::as_slice);
        let use_is_loop_header = use_preds
            .iter()
            .any(|&pred| dominators.dominates(use_loc.block, pred));

        // An allocation with observable identity cannot be repeated implicitly.
        // Merely checking whether the use block is a loop header misses the
        // common shape `header -> body(use) -> header`: sinking an allocation
        // made before that loop into its body creates a fresh object on every
        // iteration. Look for a path from the use back to itself which does not
        // cross the definition block, and keep the allocation materialized when
        // such a path exists.
        let repeats_allocation = rvalue_allocates_with_identity(&def.rvalue)
            && use_repeats_without_definition(body, def.block, use_loc.block);

        if use_is_loop_header || repeats_allocation {
            return false;
        }

        // Still check the def block (from def to end) and the use block
        // (from start to use) for same-block side effects.
        if has_side_effects_between(
            body,
            def.block,
            def_idx + 1,
            body.block(def.block).statements.len(),
            &def.rvalue,
            def_use,
        ) {
            return false;
        }

        if let StatementRef::Statement(use_idx) = use_loc.statement_ref {
            if use_idx > 0
                && has_side_effects_between(body, use_loc.block, 0, use_idx, &def.rvalue, def_use)
            {
                return false;
            }
        }
    }

    true
}

/// Whether evaluating this rvalue allocates a fresh object whose identity is
/// observable (mutable containers, class instances, and callable objects).
///
/// Matched exhaustively on purpose: a wrong `false` silently miscompiles.
fn rvalue_allocates_with_identity(rvalue: &Rvalue) -> bool {
    match rvalue {
        Rvalue::Map(..)
        | Rvalue::Array(..)
        | Rvalue::Uint8Array(_)
        | Rvalue::Aggregate { .. }
        | Rvalue::MakeClosure { .. }
        | Rvalue::MakeBoundMethod { .. }
        | Rvalue::MakeVirtualBoundMethod { .. }
        | Rvalue::MakeVirtualFunction { .. } => true,
        Rvalue::Use(_)
        | Rvalue::BinaryOp { .. }
        | Rvalue::UnaryOp { .. }
        | Rvalue::Discriminant(_)
        | Rvalue::TypeTag(_)
        | Rvalue::Len(_)
        | Rvalue::IsType { .. }
        | Rvalue::IsTypeTag { .. }
        | Rvalue::RuntimeIsType { .. }
        | Rvalue::VirtualFieldAccess { .. }
        | Rvalue::MakeGenericFunction { .. }
        | Rvalue::MakeGenericFunctionFromValue { .. }
        | Rvalue::LoadType(_)
        | Rvalue::CurrentPackage(_) => false,
    }
}

/// Whether `use_block` can execute again without first executing `def_block`.
///
/// `can_be_virtual` sinks an rvalue from its definition to its use. If a CFG
/// cycle can revisit the use while bypassing the definition, sinking changes a
/// once-evaluated binding into a per-iteration evaluation. That is observably
/// wrong for an allocation with observable identity, so its cross-block
/// virtualization must reject the shape.
fn use_repeats_without_definition(
    body: &MirFunctionBody,
    def_block: BlockId,
    use_block: BlockId,
) -> bool {
    let Some(terminator) = body.block(use_block).terminator.as_ref() else {
        return false;
    };

    let mut worklist = terminator.successors();
    let mut visited = HashSet::new();

    while let Some(block) = worklist.pop() {
        if block == def_block {
            continue;
        }
        if block == use_block {
            return true;
        }
        if !visited.insert(block) {
            continue;
        }
        if let Some(terminator) = body.block(block).terminator.as_ref() {
            worklist.extend(terminator.successors());
        }
    }

    false
}

/// Whether evaluating this rvalue reads through any field/index projection.
///
/// Cross-block virtual inlining re-evaluates the rvalue at use site. Projection
/// reads are difficult to prove safe with local-only def-use, so we conservatively
/// block cross-block virtualization when they appear.
fn rvalue_has_projection_reads(rvalue: &Rvalue) -> bool {
    fn place_has_projection(place: &Place) -> bool {
        match place {
            Place::Local(_) => false,
            Place::Capture(_) => false,
            Place::Field { .. } | Place::Index { .. } => true,
        }
    }

    fn operand_has_projection(operand: &Operand) -> bool {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => place_has_projection(place),
            Operand::Constant(_) => false,
        }
    }

    match rvalue {
        Rvalue::Use(operand) => operand_has_projection(operand),
        Rvalue::BinaryOp { left, right, .. } => {
            operand_has_projection(left) || operand_has_projection(right)
        }
        Rvalue::UnaryOp { operand, .. } => operand_has_projection(operand),
        Rvalue::Array(_, elements) => elements.iter().any(operand_has_projection),
        Rvalue::Uint8Array(_) => false,
        Rvalue::Map(_, _, entries) => entries
            .iter()
            .any(|(key, value)| operand_has_projection(key) || operand_has_projection(value)),
        Rvalue::Aggregate { fields, .. } => fields.iter().any(operand_has_projection),
        Rvalue::Discriminant(place) | Rvalue::TypeTag(place) | Rvalue::Len(place) => {
            place_has_projection(place)
        }
        Rvalue::IsType { operand, .. } | Rvalue::IsTypeTag { operand, .. } => {
            operand_has_projection(operand)
        }
        Rvalue::RuntimeIsType {
            operand,
            type_value,
        } => operand_has_projection(operand) || operand_has_projection(type_value),
        Rvalue::MakeClosure { captures, .. } => captures.iter().any(operand_has_projection),
        Rvalue::MakeBoundMethod { receiver, .. }
        | Rvalue::MakeVirtualBoundMethod { receiver, .. }
        | Rvalue::VirtualFieldAccess { receiver, .. } => operand_has_projection(receiver),
        Rvalue::MakeVirtualFunction { type_args, .. } => {
            type_args.iter().any(operand_has_projection)
        }
        Rvalue::LoadType(_) | Rvalue::CurrentPackage(_) | Rvalue::MakeGenericFunction { .. } => {
            false
        }
        Rvalue::MakeGenericFunctionFromValue { value, .. } => operand_has_projection(value),
    }
}

/// Check for side effects between two statement indices in a block.
///
/// A side effect is anything that could change the value of the rvalue when re-evaluated:
/// - Function calls (may have side effects)
/// - Assignments to variables that the rvalue reads from (transitively)
///
/// Checks the half-open range `[start, end)`.
fn has_side_effects_between(
    body: &MirFunctionBody,
    block_id: BlockId,
    start: usize,
    end: usize,
    rvalue: &Rvalue,
    def_use: &HashMap<Local, LocalDefUse>,
) -> bool {
    let block = body.block(block_id);
    // Collect transitive reads - if this rvalue reads from local X which is defined
    // as reading from local Y, we need to track both X and Y.
    // Only follow definitions that happen BEFORE start (the current statement).
    let rvalue_reads = collect_transitive_reads(rvalue, def_use, block_id, start);

    for stmt_idx in start..end {
        let stmt = &block.statements[stmt_idx];
        if has_side_effect(&stmt.kind, &rvalue_reads) {
            return true;
        }
    }

    false
}

/// Collect all locals that an rvalue reads from, transitively.
///
/// If the rvalue reads from local X, and X is defined as the result of an
/// expression that reads from Y, we include both X and Y. This is necessary
/// because inlining X will re-evaluate its definition, which reads from Y.
///
/// We only follow definitions that occur before `def_block:def_stmt_idx` to
/// avoid including dependencies on values computed later.
fn collect_transitive_reads(
    rvalue: &Rvalue,
    def_use: &HashMap<Local, LocalDefUse>,
    def_block: BlockId,
    def_stmt_idx: usize,
) -> HashSet<Local> {
    let mut locals = HashSet::new();
    let mut worklist: Vec<Local> = Vec::new();

    // First, collect direct reads
    walk_rvalue_locals(rvalue, &mut |local| worklist.push(local));

    // Then, transitively expand
    while let Some(local) = worklist.pop() {
        if locals.insert(local) {
            // New local - check if it has a definition with an rvalue we should follow
            // Only follow if the definition is in the same block AND before the current statement
            if let Some(du) = def_use.get(&local) {
                if let Some(def) = &du.def {
                    // Only follow if definition is earlier in the same block
                    // This ensures we don't include dependencies on values computed later
                    if let StatementRef::Statement(idx) = def.statement_ref {
                        if def.block == def_block && idx < def_stmt_idx {
                            walk_rvalue_locals(&def.rvalue, &mut |local| worklist.push(local));
                        }
                    }
                }
            }
        }
    }

    locals
}

/// Check if a statement has side effects that would prevent inlining.
fn has_side_effect(kind: &StatementKind, rvalue_reads: &HashSet<Local>) -> bool {
    match kind {
        StatementKind::Assign { destination, value } => {
            // Check if this assignment modifies a variable (or field/index of a variable)
            // that the rvalue reads from.
            let Some(base_local) = destination.base_local() else {
                // Capture reads are not represented in `rvalue_reads`, so a
                // capture-rooted write must conservatively block inlining.
                return true;
            };
            if rvalue_reads.contains(&base_local) {
                return true;
            }
            // All other assignments (including loading constants) are pure
            _ = value;
            false
        }
        StatementKind::Drop(_) => true,
        StatementKind::FreshCell(local) => rvalue_reads.contains(local),
        StatementKind::VizEnter(_) | StatementKind::VizExit(_) => true, // VizEnter/VizExit emit notifications
        StatementKind::Intrinsic { .. } => true, // Intrinsics emit events — observable side effect
        // A write through an interface field mutates the receiver.
        StatementKind::VirtualFieldStore { .. } => true,
        StatementKind::Nop => false,
    }
}

/// Check if an rvalue is a pure constant that can be safely duplicated.
///
/// Pure constants have no side effects and always produce the same value,
/// so they can be re-emitted at every use site even with multiple uses.
fn is_pure_constant(rvalue: &Rvalue) -> bool {
    matches!(rvalue, Rvalue::Use(Operand::Constant(_)))
}

/// Can evaluating this rvalue raise a catchable panic (`baml.panics.*`)?
///
/// Virtual emission *moves* an rvalue's evaluation from its definition to its
/// use site. That is only sound when the evaluation cannot fail: a panicking
/// evaluation is itself an observable event, so moving it past a call, a store,
/// or an exception-region boundary changes which effects run before the panic
/// and which handler receives it.
///
/// Concretely, a `defer` block's inline replay is emitted between the
/// definition and the `return` that uses it. Sinking a panicking arithmetic op
/// past that replay runs the defer body once on the way out and a second time
/// in the unwind landing pad.
///
/// Only arithmetic can fail, and only `/` fails for every operand type. The
/// rest are `int`-only failures — `float` saturates to infinity or NaN,
/// `bigint` grows, and `string + string` is concatenation — so they ask
/// [`operand_could_be_int`]. Bitwise and/or/xor and the comparisons stay in
/// range whatever the operands are.
///
/// Matched exhaustively on purpose. This is a soundness predicate, and a
/// wrong `false` miscompiles silently — so a new `Rvalue` variant must fail to
/// compile here rather than default into the infallible group.
fn rvalue_can_panic(body: &MirFunctionBody, rvalue: &Rvalue) -> bool {
    match rvalue {
        Rvalue::BinaryOp { op, left, right } => match op {
            // `/` rejects a zero divisor on both numeric paths — BAML throws
            // rather than yielding IEEE infinity (`OpCode::DivFloat`), so this
            // holds whatever the operands are.
            BinOp::Div => true,
            // `%` is guarded on the `int` path only; the float path yields NaN.
            BinOp::Mod | BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Shl | BinOp::Shr => {
                operand_could_be_int(body, left) && operand_could_be_int(body, right)
            }
            BinOp::Eq
            | BinOp::Ne
            | BinOp::Lt
            | BinOp::Le
            | BinOp::Gt
            | BinOp::Ge
            | BinOp::BitAnd
            | BinOp::BitOr
            | BinOp::BitXor => false,
        },
        Rvalue::UnaryOp { op, operand } => match op {
            UnaryOp::Neg => operand_could_be_int(body, operand),
            UnaryOp::Not | UnaryOp::Truthy => false,
        },
        // Allocation can report `AllocFailure`, but that is a host resource
        // condition rather than a property of the program point, and treating
        // every allocation as a barrier would disable virtualization outright.
        //
        // `Use` is the one entry here with a real failing case: reading through
        // an index projection can raise `IndexOutOfBounds`. Every rvalue with a
        // projection read is rejected a few lines above this predicate's only
        // caller, on the same cross-block path, so it never reaches here.
        Rvalue::Use(_)
        | Rvalue::Array(..)
        | Rvalue::Uint8Array(_)
        | Rvalue::Map(..)
        | Rvalue::Aggregate { .. }
        | Rvalue::Discriminant(_)
        | Rvalue::TypeTag(_)
        | Rvalue::Len(_)
        | Rvalue::IsType { .. }
        | Rvalue::IsTypeTag { .. }
        | Rvalue::RuntimeIsType { .. }
        | Rvalue::MakeClosure { .. }
        | Rvalue::MakeBoundMethod { .. }
        | Rvalue::MakeVirtualBoundMethod { .. }
        | Rvalue::VirtualFieldAccess { .. }
        | Rvalue::MakeGenericFunction { .. }
        | Rvalue::MakeGenericFunctionFromValue { .. }
        | Rvalue::MakeVirtualFunction { .. }
        | Rvalue::LoadType(_)
        | Rvalue::CurrentPackage(_) => false,
    }
}

/// Could this operand hold an `int` at runtime?
///
/// Deliberately answers `true` for anything whose runtime representation is not
/// pinned down — a union, a type variable, a value read through a projection, a
/// type family variant added later. Only a type that provably never holds an
/// `int` answers `false`.
fn operand_could_be_int(body: &MirFunctionBody, operand: &Operand) -> bool {
    match operand {
        Operand::Constant(c) => matches!(c, Constant::Int(_)),
        Operand::Copy(place) | Operand::Move(place) => match place {
            Place::Local(local) => ty_could_be_int(&body.local(*local).ty),
            // A field / index / capture read carries no type here.
            Place::Field { .. } | Place::Index { .. } | Place::Capture(_) => true,
        },
    }
}

/// See [`operand_could_be_int`]. The `_ => true` fallback keeps an unlisted or
/// newly added variant on the conservative side.
fn ty_could_be_int(ty: &RuntimeTy) -> bool {
    match ty {
        RuntimeTy::Int { .. } => true,
        RuntimeTy::Literal(lit, ..) => matches!(lit, Literal::Int(_)),
        RuntimeTy::Bigint { .. }
        | RuntimeTy::Float { .. }
        | RuntimeTy::String { .. }
        | RuntimeTy::Bool { .. }
        | RuntimeTy::Null { .. }
        | RuntimeTy::Void { .. }
        | RuntimeTy::Media(..)
        | RuntimeTy::Class(..)
        | RuntimeTy::Enum(..)
        | RuntimeTy::EnumVariant(..)
        | RuntimeTy::List(..)
        | RuntimeTy::Map { .. }
        | RuntimeTy::Function { .. }
        | RuntimeTy::Future(..)
        | RuntimeTy::RustType { .. }
        | RuntimeTy::Type { .. }
        | RuntimeTy::Resource { .. }
        | RuntimeTy::PromptAst { .. } => false,
        _ => true,
    }
}

/// Check if a local is a "call result immediate": defined by Call/Await/SysOp,
/// used exactly once in the continuation block.
///
/// Call result immediate applies when:
/// 1. The local is defined by a Call/Await/SysOp terminator
/// 2. It has exactly one use
/// 3. The use is in the continuation block (target of the Call)
///
/// This allows us to:
/// - After Call: don't emit `StoreVar` (leave result on stack)
/// - At use site: don't emit `LoadVar` (value already on stack from Call)
///
/// This eliminates the redundant `StoreVar("_X"); LoadVar("_X")` pattern for call results.
fn is_call_result_immediate(local: Local, du: &LocalDefUse, body: &MirFunctionBody) -> bool {
    // Must have exactly one use
    if du.uses.len() != 1 {
        return false;
    }

    // A class spread is emitted incrementally as
    // `AllocInstance; InitField/InitSpread`. Its explicit field operands must
    // be pushed after the destination instance exists. Carrying a call result
    // from the preceding block leaves it below that instance and reverses the
    // `InitField` operands. Reject this structurally here, including when the
    // aggregate destination is virtual and its use is forwarded elsewhere.
    let use_loc = &du.uses[0];
    if let StatementRef::Statement(stmt_idx) = use_loc.statement_ref
        && let Some(StatementKind::Assign {
            value:
                Rvalue::Aggregate {
                    kind: baml_compiler2_mir::AggregateKind::Class { .. },
                    fields,
                },
            ..
        }) = body
            .block(use_loc.block)
            .statements
            .get(stmt_idx)
            .map(|stmt| &stmt.kind)
        && fields.iter().any(is_class_field_copy_operand)
    {
        return false;
    }

    // Must have a definition from a terminator (Call/Await/SysOp)
    let Some(def) = &du.def else {
        return false;
    };

    // Definition must be in a terminator
    if def.statement_ref != StatementRef::Terminator {
        return false;
    }

    // Get the defining block and check that its terminator is Call/Await/SysOp
    // that defines this local.
    let def_block = body.block(def.block);
    match &def_block.terminator {
        Some(Terminator::Call { destination, .. }) => {
            matches!(destination, Place::Local(l) if *l == local)
        }
        Some(Terminator::Await { destination, .. }) => {
            matches!(destination, Place::Local(l) if *l == local)
        }
        // NOTE: `AwaitAny` is intentionally NOT treated as a call-result
        // immediate. Its opcode rewinds + re-executes across the engine
        // suspend (like `Await`), but its result also commonly feeds straight
        // into an indexed `await futures[i]`; carrying the result on the stack
        // across that combination misaligns the stack. Always store it to a
        // local instead (correct, marginally less optimal).
        //
        // `VirtualCall` is likewise excluded: its result lands on the stack like
        // `Call`, but the open-world dispatch first pushes the interface type +
        // method-name operands, and the carry-result/store-elision path is not
        // wired for that shape. Storing to a local is correct and only
        // marginally less optimal; carrying can be enabled later.
        Some(Terminator::SysOp { destination, .. }) => {
            matches!(destination, Place::Local(l) if *l == local)
        }
        _ => false,
    }
}

/// Check if a call-like result is used as part of a stack-consumable aggregate prefix.
///
/// Map and array allocation consume values in source order, so a chain like
/// `v1 = call ...; v2 = call ...; map { "a": v1, "b": v2 }` can keep `v1`
/// and `v2` on the VM stack until the final `alloc_map`.
fn is_call_result_aggregate_operand(
    local: Local,
    du: &LocalDefUse,
    body: &MirFunctionBody,
    def_use: &HashMap<Local, LocalDefUse>,
) -> bool {
    if !is_call_like_result_local(local, du, body) {
        return false;
    }

    let [use_loc] = du.uses.as_slice() else {
        return false;
    };
    let StatementRef::Statement(stmt_idx) = use_loc.statement_ref else {
        return false;
    };
    let Some(StatementKind::Assign { value, .. }) = body
        .block(use_loc.block)
        .statements
        .get(stmt_idx)
        .map(|stmt| &stmt.kind)
    else {
        return false;
    };
    let Some(operands) = aggregate_stack_prefix_operands(value) else {
        return false;
    };

    let mut found_local = false;
    for operand in operands {
        let Some(operand_local) = operand_local(operand) else {
            return false;
        };

        if operand_local == local {
            found_local = true;
            continue;
        }

        let Some(operand_du) = def_use.get(&operand_local) else {
            return false;
        };
        if !is_call_like_result_local(operand_local, operand_du, body) {
            return false;
        }
        let [operand_use] = operand_du.uses.as_slice() else {
            return false;
        };
        if operand_use.block != use_loc.block || operand_use.statement_ref != use_loc.statement_ref
        {
            return false;
        }
    }

    found_local
}

fn aggregate_stack_prefix_operands(rvalue: &Rvalue) -> Option<Vec<&Operand>> {
    match rvalue {
        Rvalue::Array(_, elements) => Some(elements.iter().collect()),
        // Map lowering emits all values first, then all keys, because the VM
        // consumes maps as `[v1, v2, ..., k1, k2, ...]`. A carried key would sit
        // below the emitted values, so only value positions are stack-carryable.
        Rvalue::Map(_, _, entries) => Some(entries.iter().map(|(_key, value)| value).collect()),
        Rvalue::Aggregate {
            kind: baml_compiler2_mir::AggregateKind::Array,
            fields,
        } => Some(fields.iter().collect()),
        Rvalue::Aggregate {
            kind: baml_compiler2_mir::AggregateKind::Class { .. },
            fields,
        } if !fields.iter().any(is_class_field_copy_operand) => Some(fields.iter().collect()),
        // Class aggregates with field-copy operands use the `init_spread` path
        // instead of the field-value init plan, so stack-carried values would
        // not be consumed in the order modeled here.
        Rvalue::Aggregate { .. } => None,
        _ => None,
    }
}

fn is_class_field_copy_operand(operand: &Operand) -> bool {
    let place = match operand {
        Operand::Copy(place) | Operand::Move(place) => place,
        Operand::Constant(_) => return false,
    };
    matches!(place, Place::Field { .. })
}

fn operand_local(operand: &Operand) -> Option<Local> {
    match operand {
        Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => Some(*local),
        _ => None,
    }
}

fn is_call_like_result_local(local: Local, du: &LocalDefUse, body: &MirFunctionBody) -> bool {
    if du.uses.len() != 1 {
        return false;
    }

    let Some(def) = &du.def else {
        return false;
    };
    if def.statement_ref != StatementRef::Terminator {
        return false;
    }

    let def_block = body.block(def.block);
    match &def_block.terminator {
        Some(
            Terminator::Call { destination, .. }
            | Terminator::Await { destination, .. }
            // `AwaitAny` deliberately excluded — see the note in the sibling
            // call-result-immediate check above.
            | Terminator::SysOp { destination, .. },
        ) => {
            matches!(destination, Place::Local(l) if *l == local)
        }
        _ => false,
    }
}

/// Check if a local is a simple copy of another local (for copy propagation).
///
/// Returns `Some(source_local)` if the local is defined as `_X = copy _Y` where:
/// 1. There is exactly one definition of `_X`
/// 2. The definition is `Rvalue::Use(Operand::Copy(Place::Local(source)))` or
///    `Rvalue::Use(Operand::Move(Place::Local(source)))`
/// 3. The source is a parameter (not modified) or another suitable local
///
/// This optimization is particularly useful for match expressions where the
/// scrutinee is copied into a temporary before comparisons.
fn get_copy_source(
    du: &LocalDefUse,
    arity: usize,
    def_use: &HashMap<Local, LocalDefUse>,
) -> Option<Local> {
    // Must have exactly one definition
    let def = du.def.as_ref()?;

    // Definition must not be from a terminator (Call/Await results aren't copies)
    if def.statement_ref == StatementRef::Terminator {
        return None;
    }

    // Must have exactly one definition site
    if du.all_defs.len() != 1 {
        return None;
    }

    // The rvalue must be a simple copy/move of a local (not a field or index)
    let source = match &def.rvalue {
        Rvalue::Use(Operand::Copy(Place::Local(src))) => *src,
        Rvalue::Use(Operand::Move(Place::Local(src))) => *src,
        _ => return None,
    };

    // The source must be a parameter that is never reassigned.
    // We only propagate copies of parameters to keep the analysis simple and safe.
    // Propagating copies of other locals would require verifying the source isn't
    // modified between the copy and all uses of the copy.
    let source_idx = source.0;
    if source_idx == 0 || source_idx > arity {
        // Source is not a parameter (_0 is return value, > arity are locals)
        return None;
    }

    // The source parameter must not be reassigned anywhere in the function.
    // BAML allows parameter mutation (e.g., `x = 3` where x is a param),
    // so we must verify the parameter has no explicit defs.
    if let Some(source_du) = def_use.get(&source) {
        if !source_du.all_defs.is_empty() {
            // Parameter is reassigned somewhere — not safe to propagate.
            return None;
        }
    }

    Some(source)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use baml_compiler2_mir::{
        BasicBlock, Constant, LocalDecl, MirFunctionBody, Operand, Place, Statement, Terminator,
    };
    use baml_type::{RuntimeTy, TyAttr};

    use super::*;

    #[test]
    fn test_dominates_entry() {
        let mut idom = HashMap::new();
        idom.insert(BlockId(0), None);
        idom.insert(BlockId(1), Some(BlockId(0)));
        idom.insert(BlockId(2), Some(BlockId(1)));

        let mut rpo_idx = HashMap::new();
        rpo_idx.insert(BlockId(0), 0);
        rpo_idx.insert(BlockId(1), 1);
        rpo_idx.insert(BlockId(2), 2);

        let doms = Dominators { idom, rpo_idx };

        // Entry dominates everything
        assert!(doms.dominates(BlockId(0), BlockId(0)));
        assert!(doms.dominates(BlockId(0), BlockId(1)));
        assert!(doms.dominates(BlockId(0), BlockId(2)));

        // bb1 dominates bb2
        assert!(doms.dominates(BlockId(1), BlockId(2)));

        // bb2 doesn't dominate bb1
        assert!(!doms.dominates(BlockId(2), BlockId(1)));
    }

    #[test]
    fn aggregate_operand_requires_all_prefix_operands_to_be_stack_carried() {
        let target = Local(1);
        let body = MirFunctionBody {
            blocks: vec![
                BasicBlock {
                    id: BlockId(0),
                    statements: vec![],
                    terminator: Some(Terminator::Call {
                        callee: Operand::Constant(Constant::Null),
                        args: vec![],
                        ntypeargs: 0,
                        runtime_type_check: false,
                        runtime_id: None,
                        destination: Place::Local(target),
                        target: BlockId(1),
                        unwind: None,
                    }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(1),
                    statements: vec![Statement {
                        kind: StatementKind::Assign {
                            destination: Place::Local(Local(0)),
                            value: Rvalue::Array(
                                baml_type::TyTemplate::from(baml_type::RealizedTy::unknown()),
                                vec![
                                    Operand::copy_local(target),
                                    Operand::Constant(Constant::Int(1)),
                                ],
                            ),
                        },
                        span: None,
                    }],
                    terminator: Some(Terminator::Return),
                    span: None,
                    terminator_span: None,
                },
            ],
            entry: BlockId(0),
            locals: vec![],
            catch_regions: vec![],
            viz_nodes: vec![],
        };
        let du = LocalDefUse {
            def: Some(DefLocation {
                block: BlockId(0),
                statement_ref: StatementRef::Terminator,
                rvalue: Rvalue::Use(Operand::Constant(Constant::Null)),
            }),
            uses: vec![UseLocation {
                block: BlockId(1),
                statement_ref: StatementRef::Statement(0),
            }],
            all_defs: vec![(BlockId(0), StatementRef::Terminator)],
        };
        let def_use = HashMap::from([(target, du.clone())]);

        assert!(!is_call_result_aggregate_operand(
            target, &du, &body, &def_use,
        ));
    }

    #[test]
    fn call_result_immediate_rejects_incremental_class_spread_init() {
        let result = Local(1);
        let spread_base = Local(2);
        let body = MirFunctionBody {
            blocks: vec![
                BasicBlock {
                    id: BlockId(0),
                    statements: vec![],
                    terminator: Some(Terminator::Call {
                        callee: Operand::Constant(Constant::Null),
                        args: vec![],
                        ntypeargs: 0,
                        runtime_type_check: false,
                        runtime_id: None,
                        destination: Place::Local(result),
                        target: BlockId(1),
                        unwind: None,
                    }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(1),
                    statements: vec![Statement {
                        kind: StatementKind::Assign {
                            destination: Place::Local(Local(0)),
                            value: Rvalue::Aggregate {
                                kind: baml_compiler2_mir::AggregateKind::Class {
                                    name: "GuideHooks".to_string(),
                                    type_arg_templates: vec![],
                                },
                                fields: vec![
                                    Operand::copy_local(result),
                                    Operand::Copy(Place::Field {
                                        base: Box::new(Place::Local(spread_base)),
                                        field: 1,
                                    }),
                                ],
                            },
                        },
                        span: None,
                    }],
                    terminator: Some(Terminator::Return),
                    span: None,
                    terminator_span: None,
                },
            ],
            entry: BlockId(0),
            locals: vec![],
            catch_regions: vec![],
            viz_nodes: vec![],
        };
        let du = LocalDefUse {
            def: Some(DefLocation {
                block: BlockId(0),
                statement_ref: StatementRef::Terminator,
                rvalue: Rvalue::Use(Operand::Constant(Constant::Null)),
            }),
            uses: vec![UseLocation {
                block: BlockId(1),
                statement_ref: StatementRef::Statement(0),
            }],
            all_defs: vec![(BlockId(0), StatementRef::Terminator)],
        };

        assert!(!is_call_result_immediate(result, &du, &body));
    }

    /// Builds a minimal integer local declaration for MIR analysis tests.
    fn int_local_decl(name: Option<&str>) -> LocalDecl {
        LocalDecl {
            name: name.map(baml_base::Name::new),
            ty: RuntimeTy::Int {
                attr: TyAttr::default(),
            },
            span: None,
            scope_span: None,
            is_captured: false,
        }
    }

    fn int_list_local_decl(name: Option<&str>) -> LocalDecl {
        LocalDecl {
            name: name.map(baml_base::Name::new),
            ty: RuntimeTy::list(RuntimeTy::int()),
            span: None,
            scope_span: None,
            is_captured: false,
        }
    }

    /// `a && b && c`: two chained `ShortCircuit` terminators whose inner join
    /// (bb3) is an empty passthrough into the outer join (bb4).
    fn nested_short_circuit_body(
        name: Option<&str>,
        with_prior_definition: bool,
    ) -> MirFunctionBody {
        let destination = Local(1);
        let prior_definition = with_prior_definition.then(|| assign_bool(destination, false));

        bool_body(
            vec![
                BasicBlock {
                    id: BlockId(0),
                    statements: prior_definition.into_iter().collect(),
                    terminator: Some(Terminator::ShortCircuit {
                        operand: Operand::Constant(Constant::Bool(true)),
                        is_and: true,
                        destination: Place::Local(destination),
                        eval_rhs: BlockId(1),
                        join: BlockId(4),
                    }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(1),
                    statements: vec![],
                    terminator: Some(Terminator::ShortCircuit {
                        operand: Operand::Constant(Constant::Bool(true)),
                        is_and: true,
                        destination: Place::Local(destination),
                        eval_rhs: BlockId(2),
                        join: BlockId(3),
                    }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(2),
                    statements: vec![assign_bool(destination, true)],
                    terminator: Some(Terminator::Goto { target: BlockId(3) }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(3),
                    statements: vec![],
                    terminator: Some(Terminator::Goto { target: BlockId(4) }),
                    span: None,
                    terminator_span: None,
                },
                return_local_block(BlockId(4), destination),
            ],
            name,
        )
    }

    fn bool_local_decl(name: Option<&str>) -> LocalDecl {
        LocalDecl {
            name: name.map(baml_base::Name::new),
            ty: RuntimeTy::Bool {
                attr: TyAttr::default(),
            },
            span: None,
            scope_span: None,
            is_captured: false,
        }
    }

    fn assign_bool(destination: Local, value: bool) -> Statement {
        Statement {
            kind: StatementKind::Assign {
                destination: Place::Local(destination),
                value: Rvalue::Use(Operand::Constant(Constant::Bool(value))),
            },
            span: None,
        }
    }

    /// `_0 = copy destination; return` — the single use of the carried local.
    fn return_local_block(id: BlockId, destination: Local) -> BasicBlock {
        BasicBlock {
            id,
            statements: vec![Statement {
                kind: StatementKind::Assign {
                    destination: Place::Local(Local(0)),
                    value: Rvalue::Use(Operand::copy_local(destination)),
                },
                span: None,
            }],
            terminator: Some(Terminator::Return),
            span: None,
            terminator_span: None,
        }
    }

    fn bool_body(blocks: Vec<BasicBlock>, name: Option<&str>) -> MirFunctionBody {
        MirFunctionBody {
            blocks,
            entry: BlockId(0),
            locals: vec![bool_local_decl(None), bool_local_decl(name)],
            catch_regions: vec![],
            viz_nodes: vec![],
        }
    }

    fn is_stack_covered(body: &MirFunctionBody, local: Local) -> bool {
        let def_use = collect_def_use(body);
        let predecessors = build_predecessors(body);

        is_stack_covered_phi(local, &def_use[&local], body, &predecessors)
    }

    #[test]
    fn nested_short_circuit_definitions_are_stack_carried() {
        let body = nested_short_circuit_body(None, false);

        assert!(is_stack_covered(&body, Local(1)));
    }

    /// A user-named local is not special: what matters is that every edge into
    /// the use block pushes its value, and that nothing else defines it.
    #[test]
    fn named_short_circuit_local_is_stack_carried() {
        let body = nested_short_circuit_body(Some("result"), false);

        assert!(is_stack_covered(&body, Local(1)));
    }

    #[test]
    fn reassigned_named_short_circuit_local_is_materialized() {
        let body = nested_short_circuit_body(Some("result"), true);

        assert!(!is_stack_covered(&body, Local(1)));
    }

    /// The stray initializer is just as unbalanced when the local is a compiler
    /// temp, so the name plays no part in rejecting it.
    #[test]
    fn reassigned_unnamed_short_circuit_local_is_materialized() {
        let body = nested_short_circuit_body(None, true);

        assert!(!is_stack_covered(&body, Local(1)));
    }

    /// `let x = false; if (c) { x = a && b } x` — MIR merges the short circuit's
    /// own join into the `if` join, so the `ShortCircuit`'s `join` really is the
    /// use block. The `Branch` false edge still reaches that block without
    /// pushing anything, so the local has to stay in its slot.
    fn merged_join_body(name: Option<&str>) -> MirFunctionBody {
        let destination = Local(1);

        bool_body(
            vec![
                BasicBlock {
                    id: BlockId(0),
                    statements: vec![],
                    terminator: Some(Terminator::Branch {
                        condition: Operand::Constant(Constant::Bool(true)),
                        then_block: BlockId(1),
                        else_block: BlockId(3),
                    }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(1),
                    statements: vec![],
                    terminator: Some(Terminator::ShortCircuit {
                        operand: Operand::Constant(Constant::Bool(true)),
                        is_and: true,
                        destination: Place::Local(destination),
                        eval_rhs: BlockId(2),
                        join: BlockId(3),
                    }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(2),
                    statements: vec![assign_bool(destination, true)],
                    terminator: Some(Terminator::Goto { target: BlockId(3) }),
                    span: None,
                    terminator_span: None,
                },
                return_local_block(BlockId(3), destination),
            ],
            name,
        )
    }

    #[test]
    fn short_circuit_join_shared_with_a_branch_edge_is_materialized() {
        assert!(!is_stack_covered(&merged_join_body(Some("x")), Local(1)));
        assert!(!is_stack_covered(&merged_join_body(None), Local(1)));
    }

    /// One predecessor short-circuits into the join, the other assigns and
    /// falls through. Neither of the two predicates this replaced accepted the
    /// mix on its own.
    #[test]
    fn mixed_short_circuit_and_assignment_predecessors_are_stack_carried() {
        let destination = Local(1);
        let body = bool_body(
            vec![
                BasicBlock {
                    id: BlockId(0),
                    statements: vec![],
                    terminator: Some(Terminator::Branch {
                        condition: Operand::Constant(Constant::Bool(true)),
                        then_block: BlockId(1),
                        else_block: BlockId(3),
                    }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(1),
                    statements: vec![],
                    terminator: Some(Terminator::ShortCircuit {
                        operand: Operand::Constant(Constant::Bool(true)),
                        is_and: true,
                        destination: Place::Local(destination),
                        eval_rhs: BlockId(2),
                        join: BlockId(4),
                    }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(2),
                    statements: vec![assign_bool(destination, true)],
                    terminator: Some(Terminator::Goto { target: BlockId(4) }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(3),
                    statements: vec![assign_bool(destination, false)],
                    terminator: Some(Terminator::Goto { target: BlockId(4) }),
                    span: None,
                    terminator_span: None,
                },
                return_local_block(BlockId(4), destination),
            ],
            Some("x"),
        );

        assert!(is_stack_covered(&body, Local(1)));
    }

    /// The classic phi-like diamond: both arms assign and fall through.
    fn diamond_body(with_prior_definition: bool) -> MirFunctionBody {
        let destination = Local(1);
        let prior_definition = with_prior_definition.then(|| assign_bool(destination, false));

        bool_body(
            vec![
                BasicBlock {
                    id: BlockId(0),
                    statements: prior_definition.into_iter().collect(),
                    terminator: Some(Terminator::Branch {
                        condition: Operand::Constant(Constant::Bool(true)),
                        then_block: BlockId(1),
                        else_block: BlockId(2),
                    }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(1),
                    statements: vec![assign_bool(destination, true)],
                    terminator: Some(Terminator::Goto { target: BlockId(3) }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(2),
                    statements: vec![assign_bool(destination, false)],
                    terminator: Some(Terminator::Goto { target: BlockId(3) }),
                    span: None,
                    terminator_span: None,
                },
                return_local_block(BlockId(3), destination),
            ],
            Some("x"),
        )
    }

    #[test]
    fn assignment_in_every_predecessor_is_stack_carried() {
        assert!(is_stack_covered(&diamond_body(false), Local(1)));
    }

    /// `let x = 0; if (c) { x = 1 } else { x = 2 }; use(x)` — the initializer is
    /// emitted as a push that nothing pops, so inside a loop it grew the operand
    /// stack once per iteration.
    #[test]
    fn diamond_with_a_definition_outside_the_predecessors_is_materialized() {
        assert!(!is_stack_covered(&diamond_body(true), Local(1)));
    }

    /// A single static use in a CFG cycle represents repeated dynamic uses.
    /// Sinking the array allocation from block 0 into the call in block 1 would
    /// allocate a fresh array on every trip around the cycle.
    #[test]
    fn repeated_identity_allocation_is_not_virtualized() {
        let array = Local(1);
        let call_result = Local(2);
        let body = MirFunctionBody {
            blocks: vec![
                BasicBlock {
                    id: BlockId(0),
                    statements: vec![Statement {
                        kind: StatementKind::Assign {
                            destination: Place::Local(array),
                            value: Rvalue::Array(
                                baml_type::TyTemplate::from(baml_type::RealizedTy::int()),
                                vec![],
                            ),
                        },
                        span: None,
                    }],
                    terminator: Some(Terminator::Goto { target: BlockId(1) }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(1),
                    statements: vec![],
                    terminator: Some(Terminator::Call {
                        callee: Operand::Constant(Constant::Null),
                        args: vec![Operand::copy_local(array)],
                        ntypeargs: 0,
                        runtime_type_check: false,
                        runtime_id: None,
                        destination: Place::Local(call_result),
                        target: BlockId(2),
                        unwind: None,
                    }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(2),
                    statements: vec![],
                    terminator: Some(Terminator::Goto { target: BlockId(1) }),
                    span: None,
                    terminator_span: None,
                },
            ],
            entry: BlockId(0),
            locals: vec![
                int_local_decl(None),
                int_list_local_decl(Some("items")),
                int_local_decl(Some("result")),
            ],
            catch_regions: vec![],
            viz_nodes: vec![],
        };

        let analysis = AnalysisResult::analyze(&body, 0, OptLevel::One);
        assert_eq!(
            analysis.classifications.get(&array),
            Some(&LocalClassification::Real)
        );
    }

    /// The identity guard is cycle-specific: a single cross-block use that
    /// cannot repeat without re-running the definition remains virtualizable.
    #[test]
    fn non_repeating_identity_allocation_stays_virtualized() {
        let array = Local(1);
        let body = MirFunctionBody {
            blocks: vec![
                BasicBlock {
                    id: BlockId(0),
                    statements: vec![Statement {
                        kind: StatementKind::Assign {
                            destination: Place::Local(array),
                            value: Rvalue::Array(
                                baml_type::TyTemplate::from(baml_type::RealizedTy::int()),
                                vec![],
                            ),
                        },
                        span: None,
                    }],
                    terminator: Some(Terminator::Goto { target: BlockId(1) }),
                    span: None,
                    terminator_span: None,
                },
                BasicBlock {
                    id: BlockId(1),
                    statements: vec![Statement {
                        kind: StatementKind::Assign {
                            destination: Place::Local(Local(0)),
                            value: Rvalue::Use(Operand::copy_local(array)),
                        },
                        span: None,
                    }],
                    terminator: Some(Terminator::Return),
                    span: None,
                    terminator_span: None,
                },
            ],
            entry: BlockId(0),
            locals: vec![
                int_list_local_decl(None),
                int_list_local_decl(Some("items")),
            ],
            catch_regions: vec![],
            viz_nodes: vec![],
        };

        let analysis = AnalysisResult::analyze(&body, 0, OptLevel::One);
        assert_eq!(
            analysis.classifications.get(&array),
            Some(&LocalClassification::Virtual)
        );
    }

    /// Verifies `Rvalue::Len` bindings are always classified as materialized locals.
    #[test]
    fn len_bindings_are_not_virtualized() {
        let arr = Local(1);
        let len = Local(2);
        let body = MirFunctionBody {
            blocks: vec![BasicBlock {
                id: BlockId(0),
                statements: vec![
                    Statement {
                        kind: StatementKind::Assign {
                            destination: Place::Local(arr),
                            value: Rvalue::Use(Operand::Constant(Constant::Null)),
                        },
                        span: None,
                    },
                    Statement {
                        kind: StatementKind::Assign {
                            destination: Place::Local(len),
                            value: Rvalue::Len(Place::Local(arr)),
                        },
                        span: None,
                    },
                    Statement {
                        kind: StatementKind::Assign {
                            destination: Place::Local(Local(0)),
                            value: Rvalue::Use(Operand::copy_local(len)),
                        },
                        span: None,
                    },
                ],
                terminator: Some(Terminator::Return),
                span: None,
                terminator_span: None,
            }],
            entry: BlockId(0),
            locals: vec![
                int_local_decl(None),
                int_local_decl(Some("arr")),
                int_local_decl(Some("n")),
            ],
            catch_regions: vec![],
            viz_nodes: vec![],
        };

        let analysis = AnalysisResult::analyze(&body, 0, OptLevel::One);
        assert_eq!(
            analysis.classifications.get(&len),
            Some(&LocalClassification::Real)
        );
    }
}
