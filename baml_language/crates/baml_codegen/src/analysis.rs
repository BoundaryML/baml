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

use baml_mir::{
    BlockId, Constant, Local, MirFunction, Operand, Place, Rvalue, StatementKind, Terminator,
};

// ============================================================================
// Data Structures
// ============================================================================

/// Where a local is defined.
#[derive(Clone, Debug)]
pub(crate) struct DefLocation<'db> {
    pub block: BlockId,
    pub statement_idx: usize,
    /// The rvalue that produces this local's value (for inlining).
    pub rvalue: Rvalue<'db>,
}

/// Where a local is used.
#[derive(Clone, Debug)]
pub(crate) struct UseLocation {
    pub block: BlockId,
    pub statement_idx: usize,
}

/// Sentinel value for uses in terminators.
pub(crate) const TERMINATOR_IDX: usize = usize::MAX;

/// Def-use information for a single local.
#[derive(Clone, Debug)]
pub(crate) struct LocalDefUse<'db> {
    #[allow(dead_code)] // Kept for debugging and future use
    pub local: Local,
    /// Definition site (None for parameters, which are defined at entry).
    pub def: Option<DefLocation<'db>>,
    /// All use sites.
    pub uses: Vec<UseLocation>,
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
    /// Call result immediate: defined by Call/Await/DispatchFuture, used exactly once
    /// immediately in the continuation block.
    /// At def site (after Call): don't emit Store (leave on stack).
    /// At use site: don't emit `LoadVar` (value already on stack from Call).
    CallResultImmediate,
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
    /// Reverse postorder indices for faster intersection.
    #[allow(dead_code)] // Used in dominator computation, kept for future optimizations
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
pub(crate) struct AnalysisResult<'db> {
    /// Classification for each local.
    pub classifications: HashMap<Local, LocalClassification>,
    /// Def-use information for each local.
    pub def_use: HashMap<Local, LocalDefUse<'db>>,
    /// Dominator tree.
    #[allow(dead_code)] // Kept for future scope-aware codegen
    pub dominators: Dominators,
    /// Reverse postorder of blocks (for iteration).
    pub rpo: Vec<BlockId>,
    /// Predecessor map for each block.
    #[allow(dead_code)] // Kept for future scope-aware codegen
    pub predecessors: HashMap<BlockId, Vec<BlockId>>,
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

impl<'db> AnalysisResult<'db> {
    /// Analyze a MIR function and produce classification results.
    pub(crate) fn analyze(mir: &MirFunction<'db>) -> Self {
        // Step 1: Build predecessor map
        let predecessors = build_predecessors(mir);

        // Step 2: Compute reverse postorder
        let rpo = compute_rpo(mir);

        // Step 3: Compute dominators
        let dominators = compute_dominators(mir, &rpo, &predecessors);

        // Step 4: Collect def-use information
        let def_use = collect_def_use(mir);

        // Step 5: Build jump threading redirect map
        let redirect_targets = build_redirect_targets(mir);

        // Step 6: Classify each local (including phi-like detection and copy propagation)
        let (classifications, copy_sources) =
            classify_locals(mir, &def_use, &dominators, &predecessors, &redirect_targets);

        Self {
            classifications,
            def_use,
            dominators,
            rpo,
            predecessors,
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
fn build_predecessors(mir: &MirFunction<'_>) -> HashMap<BlockId, Vec<BlockId>> {
    let mut preds: HashMap<BlockId, Vec<BlockId>> = HashMap::new();

    // Initialize with empty vecs
    for block in &mir.blocks {
        preds.insert(block.id, Vec::new());
    }

    // Collect predecessor edges from terminators
    for block in &mir.blocks {
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
    mir: &MirFunction<'_>,
    block_id: BlockId,
    visited: &mut HashSet<BlockId>,
    postorder: &mut Vec<BlockId>,
) {
    if visited.contains(&block_id) {
        return;
    }
    visited.insert(block_id);

    let block = mir.block(block_id);
    if let Some(term) = &block.terminator {
        for succ in term.successors() {
            rpo_dfs(mir, succ, visited, postorder);
        }
    }
    postorder.push(block_id);
}

/// Compute reverse postorder (depth-first, postorder reversed).
fn compute_rpo(mir: &MirFunction<'_>) -> Vec<BlockId> {
    let mut visited = HashSet::new();
    let mut postorder = Vec::new();

    rpo_dfs(mir, mir.entry, &mut visited, &mut postorder);
    postorder.reverse();
    postorder
}

// ============================================================================
// Jump Threading
// ============================================================================

/// Build redirect targets for jump threading.
///
/// Identifies empty blocks that only contain a Goto terminator and maps them
/// to their final destination. This allows emission to skip intermediate jumps.
fn build_redirect_targets(mir: &MirFunction<'_>) -> HashMap<BlockId, BlockId> {
    // First pass: identify empty goto-only blocks
    let mut goto_targets: HashMap<BlockId, BlockId> = HashMap::new();

    for block in &mir.blocks {
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

// ============================================================================
// Dominator Computation (Cooper-Harvey-Kennedy Algorithm)
// ============================================================================

/// Compute dominators using the Cooper-Harvey-Kennedy algorithm.
///
/// This is a simple, efficient iterative algorithm that computes immediate
/// dominators by repeatedly intersecting dominator sets until convergence.
fn compute_dominators(
    mir: &MirFunction<'_>,
    rpo: &[BlockId],
    preds: &HashMap<BlockId, Vec<BlockId>>,
) -> Dominators {
    // Map BlockId to RPO index for faster lookup
    let rpo_idx: HashMap<BlockId, usize> = rpo.iter().enumerate().map(|(i, &b)| (b, i)).collect();

    let mut idom: HashMap<BlockId, Option<BlockId>> = HashMap::new();

    // Initialize: entry dominates itself (represented as None for "no parent")
    idom.insert(mir.entry, None);

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
fn collect_def_use<'db>(mir: &MirFunction<'db>) -> HashMap<Local, LocalDefUse<'db>> {
    let mut def_use: HashMap<Local, LocalDefUse<'db>> = HashMap::new();

    // Initialize for all locals
    for (idx, _) in mir.locals.iter().enumerate() {
        let local = Local(idx);
        def_use.insert(
            local,
            LocalDefUse {
                local,
                def: None,
                uses: Vec::new(),
            },
        );
    }

    // Walk all blocks
    for block in &mir.blocks {
        // Walk statements
        for (stmt_idx, stmt) in block.statements.iter().enumerate() {
            match &stmt.kind {
                StatementKind::Assign { destination, value } => {
                    // Record definition
                    if let Place::Local(local) = destination {
                        if let Some(du) = def_use.get_mut(local) {
                            du.def = Some(DefLocation {
                                block: block.id,
                                statement_idx: stmt_idx,
                                rvalue: value.clone(),
                            });
                        }
                    }

                    // For field/index stores, the base local (and index local for Index) is also
                    // used. We need to load them to store the value. This ensures they aren't
                    // classified as Virtual.
                    match destination {
                        Place::Field { base, .. } => {
                            collect_uses_in_place(base, block.id, stmt_idx, &mut def_use);
                        }
                        Place::Index { base, index, .. } => {
                            collect_uses_in_place(base, block.id, stmt_idx, &mut def_use);
                            // The index is also used - we need to load it for the Store*Element
                            def_use
                                .entry(*index)
                                .or_insert_with(|| LocalDefUse {
                                    local: *index,
                                    def: None,
                                    uses: Vec::new(),
                                })
                                .uses
                                .push(UseLocation {
                                    block: block.id,
                                    statement_idx: stmt_idx,
                                });
                        }
                        Place::Local(_) => {}
                    }

                    // Record uses in the rvalue
                    collect_uses_in_rvalue(value, block.id, stmt_idx, &mut def_use);
                }
                StatementKind::Drop(place) => {
                    collect_uses_in_place(place, block.id, stmt_idx, &mut def_use);
                }
                StatementKind::Unwatch(local) => {
                    // Unwatch uses the local (we need to read its value to unlink from watch graph)
                    def_use
                        .entry(*local)
                        .or_insert_with(|| LocalDefUse {
                            local: *local,
                            def: None,
                            uses: Vec::new(),
                        })
                        .uses
                        .push(UseLocation {
                            block: block.id,
                            statement_idx: stmt_idx,
                        });
                }
                StatementKind::NotifyBlock { .. } => {
                    // NotifyBlock doesn't use any locals - it's a pure side effect
                }
                StatementKind::WatchOptions { local, filter } => {
                    // WatchOptions uses the local and the filter operand
                    def_use
                        .entry(*local)
                        .or_insert_with(|| LocalDefUse {
                            local: *local,
                            def: None,
                            uses: Vec::new(),
                        })
                        .uses
                        .push(UseLocation {
                            block: block.id,
                            statement_idx: stmt_idx,
                        });
                    collect_uses_in_operand(filter, block.id, stmt_idx, &mut def_use);
                }
                StatementKind::WatchNotify(local) => {
                    // WatchNotify uses the local
                    def_use
                        .entry(*local)
                        .or_insert_with(|| LocalDefUse {
                            local: *local,
                            def: None,
                            uses: Vec::new(),
                        })
                        .uses
                        .push(UseLocation {
                            block: block.id,
                            statement_idx: stmt_idx,
                        });
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

    def_use
}

/// Collect uses in an rvalue.
fn collect_uses_in_rvalue<'db>(
    rvalue: &Rvalue<'db>,
    block: BlockId,
    stmt_idx: usize,
    def_use: &mut HashMap<Local, LocalDefUse<'db>>,
) {
    match rvalue {
        Rvalue::Use(operand) => {
            collect_uses_in_operand(operand, block, stmt_idx, def_use);
        }
        Rvalue::BinaryOp { left, right, .. } => {
            collect_uses_in_operand(left, block, stmt_idx, def_use);
            collect_uses_in_operand(right, block, stmt_idx, def_use);
        }
        Rvalue::UnaryOp { operand, .. } => {
            collect_uses_in_operand(operand, block, stmt_idx, def_use);
        }
        Rvalue::Array(elements) => {
            for elem in elements {
                collect_uses_in_operand(elem, block, stmt_idx, def_use);
            }
        }
        Rvalue::Map(entries) => {
            for (key, value) in entries {
                collect_uses_in_operand(key, block, stmt_idx, def_use);
                collect_uses_in_operand(value, block, stmt_idx, def_use);
            }
        }
        Rvalue::Aggregate { fields, .. } => {
            for field in fields {
                collect_uses_in_operand(field, block, stmt_idx, def_use);
            }
        }
        Rvalue::Discriminant(place) | Rvalue::Len(place) => {
            collect_uses_in_place(place, block, stmt_idx, def_use);
        }
        Rvalue::IsType { operand, .. } => {
            collect_uses_in_operand(operand, block, stmt_idx, def_use);
        }
    }
}

/// Collect uses in an operand.
fn collect_uses_in_operand<'db>(
    operand: &Operand<'db>,
    block: BlockId,
    stmt_idx: usize,
    def_use: &mut HashMap<Local, LocalDefUse<'db>>,
) {
    match operand {
        Operand::Copy(place) | Operand::Move(place) => {
            collect_uses_in_place(place, block, stmt_idx, def_use);
        }
        Operand::Constant(_) => {}
    }
}

/// Collect uses in a place.
///
/// This recursively walks the place structure to find all used locals,
/// including index locals in nested `Place::Index` projections.
fn collect_uses_in_place(
    place: &Place,
    block: BlockId,
    stmt_idx: usize,
    def_use: &mut HashMap<Local, LocalDefUse<'_>>,
) {
    match place {
        Place::Local(local) => {
            // Base case: a simple local reference
            if let Some(du) = def_use.get_mut(local) {
                du.uses.push(UseLocation {
                    block,
                    statement_idx: stmt_idx,
                });
            }
        }
        Place::Field { base, .. } => {
            // Recurse into the base to find all used locals
            collect_uses_in_place(base, block, stmt_idx, def_use);
        }
        Place::Index { base, index, .. } => {
            // Recurse into the base to find all used locals
            collect_uses_in_place(base, block, stmt_idx, def_use);
            // The index local is also used
            if let Some(du) = def_use.get_mut(index) {
                du.uses.push(UseLocation {
                    block,
                    statement_idx: stmt_idx,
                });
            }
        }
    }
}

/// Collect uses (and defs for Call/Await) in a terminator.
fn collect_uses_in_terminator<'db>(
    term: &Terminator<'db>,
    block: BlockId,
    def_use: &mut HashMap<Local, LocalDefUse<'db>>,
) {
    match term {
        Terminator::Goto { .. } | Terminator::Unreachable => {}
        Terminator::Return => {
            // Return implicitly uses _0 (the return value local)
            let return_local = Local(0);
            if let Some(du) = def_use.get_mut(&return_local) {
                du.uses.push(UseLocation {
                    block,
                    statement_idx: TERMINATOR_IDX,
                });
            }
        }
        Terminator::Branch { condition, .. } => {
            collect_uses_in_operand(condition, block, TERMINATOR_IDX, def_use);
        }
        Terminator::Switch { discriminant, .. } => {
            collect_uses_in_operand(discriminant, block, TERMINATOR_IDX, def_use);
        }
        Terminator::Call {
            callee,
            args,
            destination,
            ..
        } => {
            collect_uses_in_operand(callee, block, TERMINATOR_IDX, def_use);
            for arg in args {
                collect_uses_in_operand(arg, block, TERMINATOR_IDX, def_use);
            }
            // Record the def for the destination (where call result is stored)
            if let Place::Local(local) = destination {
                if let Some(du) = def_use.get_mut(local) {
                    // For Call terminators, we use a synthetic Rvalue::Use with a placeholder
                    // The actual value comes from the call, but for classification purposes,
                    // we just need to know there's a def here
                    du.def = Some(DefLocation {
                        block,
                        statement_idx: TERMINATOR_IDX,
                        rvalue: Rvalue::Use(Operand::Constant(Constant::Null)),
                    });
                }
            }
        }
        Terminator::DispatchFuture {
            callee,
            args,
            future,
            ..
        } => {
            collect_uses_in_operand(callee, block, TERMINATOR_IDX, def_use);
            for arg in args {
                collect_uses_in_operand(arg, block, TERMINATOR_IDX, def_use);
            }
            // Record the def for the future place
            if let Place::Local(local) = future {
                if let Some(du) = def_use.get_mut(local) {
                    du.def = Some(DefLocation {
                        block,
                        statement_idx: TERMINATOR_IDX,
                        rvalue: Rvalue::Use(Operand::Constant(Constant::Null)),
                    });
                }
            }
        }
        Terminator::Await {
            future,
            destination,
            ..
        } => {
            collect_uses_in_place(future, block, TERMINATOR_IDX, def_use);
            // Record the def for the destination
            if let Place::Local(local) = destination {
                if let Some(du) = def_use.get_mut(local) {
                    du.def = Some(DefLocation {
                        block,
                        statement_idx: TERMINATOR_IDX,
                        rvalue: Rvalue::Use(Operand::Constant(Constant::Null)),
                    });
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
fn classify_locals<'db>(
    mir: &MirFunction<'db>,
    def_use: &HashMap<Local, LocalDefUse<'db>>,
    dominators: &Dominators,
    predecessors: &HashMap<BlockId, Vec<BlockId>>,
    redirect_targets: &HashMap<BlockId, BlockId>,
) -> (HashMap<Local, LocalClassification>, HashMap<Local, Local>) {
    let all_defs = collect_all_definitions(mir);

    let mut classifications = HashMap::new();
    let mut copy_sources: HashMap<Local, Local> = HashMap::new();

    for (idx, _local_decl) in mir.locals.iter().enumerate() {
        let local = Local(idx);
        let du = &def_use[&local];

        let local_decl = mir.local(local);

        // Check if this is an unused wildcard binding.
        // NOTE: We currently only check for exactly "_". In the future, we may want
        // more robust checking (e.g., any name starting with "_", or type-based analysis
        // to verify the binding truly has no observable side effects). For now, this
        // simple check handles the common pattern-matching wildcard case.
        let is_unused_wildcard = du.uses.is_empty() && local_decl.name.as_deref() == Some("_");

        let classification = if local_decl.is_watched {
            // Watched variables must always be Real - no optimizations allowed.
            // This ensures they have a stable stack slot for Watch/Unwatch instructions.
            LocalClassification::Real
        } else if idx > 0 && idx <= mir.arity {
            // Parameters are always real (they come from the caller)
            LocalClassification::Parameter
        } else if idx != 0
            && du.uses.is_empty()
            && (local_decl.name.is_none() || is_unused_wildcard)
        {
            // Dead local: either an unused compiler temp, or an unused wildcard binding.
            // Skip _0 which is implicitly used by return.
            LocalClassification::Dead
        } else if let Some(source) = get_copy_source(local, du, mir, &all_defs) {
            // Copy propagation: this local is just `_X = copy _Y` where _Y is suitable.
            // We can eliminate _X and use _Y directly at all use sites.
            copy_sources.insert(local, source);
            LocalClassification::CopyOf
        } else if can_be_virtual(local, du, dominators, mir, def_use, predecessors, &all_defs) {
            LocalClassification::Virtual
        } else if is_phi_like(local, du, mir, predecessors, &all_defs) {
            // Phi-like: assigned in each predecessor, used once at join point.
            // At def sites: emit rvalue but NOT StoreVar (leave on stack).
            // At use site: don't emit LoadVar (value already on stack).
            LocalClassification::PhiLike
        } else if is_return_phi(local, mir, &all_defs, redirect_targets) {
            // Return-phi: _0 is assigned immediately before Return in each defining block.
            // At def sites: emit rvalue but NOT StoreVar (leave on stack).
            // At Return: don't emit LoadVar for _0 (value already on stack).
            LocalClassification::ReturnPhi
        } else if is_call_result_immediate(local, du, mir) {
            // Call result used immediately in continuation block.
            // At def site (after Call): don't emit StoreVar (leave on stack).
            // At use site: don't emit LoadVar (value already on stack from Call).
            LocalClassification::CallResultImmediate
        } else {
            LocalClassification::Real
        };

        classifications.insert(local, classification);
    }

    (classifications, copy_sources)
}

/// Collect all definition sites for each local.
///
/// Unlike `def_use` which only tracks the "last" definition, this tracks ALL
/// assignments to each local across all blocks.
fn collect_all_definitions(mir: &MirFunction<'_>) -> HashMap<Local, Vec<(BlockId, usize)>> {
    let mut all_defs: HashMap<Local, Vec<(BlockId, usize)>> = HashMap::new();

    for block in &mir.blocks {
        // Collect definitions from statements
        for (stmt_idx, stmt) in block.statements.iter().enumerate() {
            if let StatementKind::Assign {
                destination: Place::Local(local),
                ..
            } = &stmt.kind
            {
                all_defs
                    .entry(*local)
                    .or_default()
                    .push((block.id, stmt_idx));
            }
        }

        // Collect definitions from terminators (Call, DispatchFuture, Await)
        if let Some(terminator) = &block.terminator {
            let dest_local = match terminator {
                Terminator::Call {
                    destination: Place::Local(local),
                    ..
                } => Some(*local),
                Terminator::DispatchFuture {
                    future: Place::Local(local),
                    ..
                } => Some(*local),
                Terminator::Await {
                    destination: Place::Local(local),
                    ..
                } => Some(*local),
                _ => None,
            };
            if let Some(local) = dest_local {
                all_defs
                    .entry(local)
                    .or_default()
                    .push((block.id, TERMINATOR_IDX));
            }
        }
    }

    all_defs
}

/// Check if a local is "phi-like": assigned in each predecessor of a join block,
/// used exactly once at that join block.
///
/// Phi-like locals can skip Store/Load because:
/// - Each predecessor leaves the value on the stack
/// - At the join point, the value is already on top of the stack
/// - No need for explicit Store/Load through a named variable
fn is_phi_like(
    local: Local,
    du: &LocalDefUse<'_>,
    mir: &MirFunction<'_>,
    predecessors: &HashMap<BlockId, Vec<BlockId>>,
    all_defs: &HashMap<Local, Vec<(BlockId, usize)>>,
) -> bool {
    // Must have exactly one use
    if du.uses.len() != 1 {
        return false;
    }

    let use_loc = &du.uses[0];
    let use_block = use_loc.block;

    // Get predecessors of the use block
    let preds = match predecessors.get(&use_block) {
        Some(p) if !p.is_empty() => p,
        _ => return false,
    };

    // Need at least 2 predecessors for this to be a join point
    if preds.len() < 2 {
        return false;
    }

    // Get all definitions of this local
    let Some(defs) = all_defs.get(&local) else {
        return false;
    };

    // Check that each predecessor:
    // 1. Defines this local
    // 2. The definition is the last statement in the block
    // 3. The block ends with Goto to the use block
    for &pred_id in preds {
        let pred_block = mir.block(pred_id);

        // Must end with Goto to use_block
        let goes_to_use_block = matches!(
            &pred_block.terminator,
            Some(Terminator::Goto { target }) if *target == use_block
        );
        if !goes_to_use_block {
            return false;
        }

        // Must have at least one statement
        if pred_block.statements.is_empty() {
            return false;
        }

        // Last statement must be an assignment to this local
        let last_stmt_idx = pred_block.statements.len() - 1;
        let last_stmt = &pred_block.statements[last_stmt_idx];

        let assigns_local = matches!(
            &last_stmt.kind,
            StatementKind::Assign { destination: Place::Local(l), .. } if *l == local
        );
        if !assigns_local {
            return false;
        }

        // Verify this definition is in our defs list
        let has_def = defs
            .iter()
            .any(|&(b, s)| b == pred_id && s == last_stmt_idx);
        if !has_def {
            return false;
        }
    }

    true
}

/// Check if `_0` (the return place) is a "return-phi" local.
///
/// Return-phi applies when `_0` is assigned immediately before Return in each defining block.
/// This allows us to:
/// - At def sites: emit rvalue but NOT `StoreVar` (leave value on stack)
/// - At Return: skip `LoadVar` for _0 (value already on stack)
///
/// This eliminates the redundant `StoreVar("_0"); LoadVar("_0"); Return` pattern.
fn is_return_phi(
    local: Local,
    mir: &MirFunction<'_>,
    all_defs: &HashMap<Local, Vec<(BlockId, usize)>>,
    redirect_targets: &HashMap<BlockId, BlockId>,
) -> bool {
    // Only applies to _0 (the return place)
    if local.0 != 0 {
        return false;
    }

    // Get all definitions of _0
    let Some(defs) = all_defs.get(&local) else {
        return false;
    };

    // Must have at least one definition
    if defs.is_empty() {
        return false;
    }

    // Build a set of return-only blocks (empty statements + Return terminator)
    let return_only_blocks: HashSet<BlockId> = mir
        .blocks
        .iter()
        .filter(|b| b.statements.is_empty() && matches!(b.terminator, Some(Terminator::Return)))
        .map(|b| b.id)
        .collect();

    // Helper: resolve a target through the redirect chain
    let resolve_target =
        |target: BlockId| -> BlockId { redirect_targets.get(&target).copied().unwrap_or(target) };

    // Each definition block must:
    // 1. Have the definition as the last statement (or be a terminator definition)
    // 2. End with Return OR Goto/Call to a return-only block (after following redirects)
    for &(block_id, stmt_idx) in defs {
        let block = mir.block(block_id);

        // Handle terminator definitions (Call, DispatchFuture, Await)
        if stmt_idx == TERMINATOR_IDX {
            // For terminator definitions, check if the continuation is return-only
            let continuation = match &block.terminator {
                Some(Terminator::Call { target, .. }) => Some(*target),
                Some(Terminator::DispatchFuture { resume, .. }) => Some(*resume),
                Some(Terminator::Await { target, .. }) => Some(*target),
                _ => None,
            };
            let valid = continuation.is_some_and(|target| {
                let resolved = resolve_target(target);
                return_only_blocks.contains(&resolved)
            });
            if !valid {
                return false;
            }
            continue;
        }

        // For regular Assign statements: definition must be the last statement
        if stmt_idx + 1 != block.statements.len() {
            return false;
        }

        // Block must end with Return or Goto to return-only block
        // Follow redirect chain for jump threading
        let valid_terminator = match &block.terminator {
            Some(Terminator::Return) => true,
            Some(Terminator::Goto { target }) => {
                let resolved = resolve_target(*target);
                return_only_blocks.contains(&resolved)
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
fn can_be_virtual<'db>(
    local: Local,
    du: &LocalDefUse<'db>,
    dominators: &Dominators,
    mir: &MirFunction<'db>,
    def_use: &HashMap<Local, LocalDefUse<'db>>,
    predecessors: &HashMap<BlockId, Vec<BlockId>>,
    all_defs: &HashMap<Local, Vec<(BlockId, usize)>>,
) -> bool {
    // Must have exactly one definition
    let Some(def) = &du.def else {
        return false;
    };

    // Definitions in terminators (Call/Await/DispatchFuture) cannot be inlined
    // because the value comes from the operation itself, not from a re-emittable rvalue
    if def.statement_idx == TERMINATOR_IDX {
        return false;
    }

    // Pure constants with a SINGLE definition can be inlined even with multiple uses.
    // They have no side effects and always produce the same value.
    // If there are multiple definitions (e.g., from if-else branches), we can't inline
    // because we'd inline the wrong definition for some execution paths.
    let has_single_def = all_defs.get(&local).is_some_and(|defs| defs.len() == 1);
    if has_single_def && is_pure_constant(&def.rvalue) {
        // Just need at least one use to not be dead
        return !du.uses.is_empty();
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

    // If in same block, use must come after def
    if def.block == use_loc.block {
        // Handle terminator uses (TERMINATOR_IDX)
        if use_loc.statement_idx == TERMINATOR_IDX {
            // Terminator always comes after all statements, so this is fine
            // But check for side effects between def and end of block
            if has_side_effects_between(
                mir,
                def.block,
                def.statement_idx,
                mir.block(def.block).statements.len(),
                &def.rvalue,
                def_use,
            ) {
                return false;
            }
        } else if use_loc.statement_idx <= def.statement_idx {
            return false;
        } else {
            // Check for intervening side effects
            if has_side_effects_between(
                mir,
                def.block,
                def.statement_idx,
                use_loc.statement_idx,
                &def.rvalue,
                def_use,
            ) {
                return false;
            }
        }
    } else {
        // Cross-block def-use
        // Check if the use block is a loop header (has back-edge predecessors)
        // If so, be conservative and don't inline
        let use_preds = predecessors
            .get(&use_loc.block)
            .map_or(&[] as &[_], |v| v.as_slice());

        // Check for back-edges: a predecessor that the use block dominates
        // indicates a loop, which means the code between def and use might
        // execute multiple times
        let has_back_edge = use_preds
            .iter()
            .any(|&pred| dominators.dominates(use_loc.block, pred));

        if has_back_edge {
            // Loop detected - be conservative
            return false;
        }

        // For non-loop cross-block, we're more permissive:
        // The rvalue will be re-evaluated at the use site, so we need to ensure
        // no intervening modifications. Check the def block from def to end.
        if has_side_effects_between(
            mir,
            def.block,
            def.statement_idx,
            mir.block(def.block).statements.len(),
            &def.rvalue,
            def_use,
        ) {
            return false;
        }

        // Check the use block from start to use
        if use_loc.statement_idx != TERMINATOR_IDX
            && use_loc.statement_idx > 0
            && has_side_effects_between(
                mir,
                use_loc.block,
                0,
                use_loc.statement_idx,
                &def.rvalue,
                def_use,
            )
        {
            return false;
        }

        // Note: We're NOT checking intermediate blocks between def and use.
        // This is safe because:
        // 1. Def dominates use (checked earlier)
        // 2. No back-edges to use block (checked above)
        // 3. The transitive read set includes all dependencies
        // If any intermediate block modified a dependency, that would require
        // the variable to appear on a path the VM takes, but since we only
        // inline single-use variables, the value at use is the value at def.
    }

    // Check if the rvalue is inlinable
    is_inlinable_rvalue(&def.rvalue)
}

/// Check for side effects between two statement indices in a block.
///
/// A side effect is anything that could change the value of the rvalue when re-evaluated:
/// - Function calls (may have side effects)
/// - Assignments to variables that the rvalue reads from (transitively)
fn has_side_effects_between<'db>(
    mir: &MirFunction<'db>,
    block_id: BlockId,
    start: usize,
    end: usize,
    rvalue: &Rvalue<'db>,
    def_use: &HashMap<Local, LocalDefUse<'db>>,
) -> bool {
    let block = mir.block(block_id);
    // Collect transitive reads - if this rvalue reads from local X which is defined
    // as reading from local Y, we need to track both X and Y.
    // Only follow definitions that happen BEFORE start (the current statement).
    let rvalue_reads = collect_transitive_reads(rvalue, def_use, block_id, start);

    for stmt_idx in (start + 1)..end {
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
fn collect_transitive_reads<'db>(
    rvalue: &Rvalue<'db>,
    def_use: &HashMap<Local, LocalDefUse<'db>>,
    def_block: BlockId,
    def_stmt_idx: usize,
) -> HashSet<Local> {
    let mut locals = HashSet::new();
    let mut worklist: Vec<Local> = Vec::new();

    // First, collect direct reads
    collect_rvalue_reads(rvalue, &mut worklist);

    // Then, transitively expand
    while let Some(local) = worklist.pop() {
        if locals.insert(local) {
            // New local - check if it has a definition with an rvalue we should follow
            // Only follow if the definition is in the same block AND before the current statement
            if let Some(du) = def_use.get(&local) {
                if let Some(def) = &du.def {
                    // Only follow if definition is earlier in the same block
                    // This ensures we don't include dependencies on values computed later
                    if def.block == def_block && def.statement_idx < def_stmt_idx {
                        collect_rvalue_reads(&def.rvalue, &mut worklist);
                    }
                }
            }
        }
    }

    locals
}

/// Collect locals directly read by an rvalue (non-transitive).
fn collect_rvalue_reads(rvalue: &Rvalue<'_>, locals: &mut Vec<Local>) {
    match rvalue {
        Rvalue::Use(operand) => collect_operand_reads(operand, locals),
        Rvalue::BinaryOp { left, right, .. } => {
            collect_operand_reads(left, locals);
            collect_operand_reads(right, locals);
        }
        Rvalue::UnaryOp { operand, .. } => {
            collect_operand_reads(operand, locals);
        }
        Rvalue::Array(elements) => {
            for op in elements {
                collect_operand_reads(op, locals);
            }
        }
        Rvalue::Map(entries) => {
            for (key, value) in entries {
                collect_operand_reads(key, locals);
                collect_operand_reads(value, locals);
            }
        }
        Rvalue::Aggregate { fields, .. } => {
            for op in fields {
                collect_operand_reads(op, locals);
            }
        }
        Rvalue::Discriminant(place) | Rvalue::Len(place) => {
            collect_place_reads(place, locals);
        }
        Rvalue::IsType { operand, .. } => {
            collect_operand_reads(operand, locals);
        }
    }
}

/// Collect locals read by an operand.
fn collect_operand_reads(operand: &Operand<'_>, locals: &mut Vec<Local>) {
    match operand {
        Operand::Copy(place) | Operand::Move(place) => {
            collect_place_reads(place, locals);
        }
        Operand::Constant(_) => {}
    }
}

/// Collect locals read by a place.
fn collect_place_reads(place: &Place, locals: &mut Vec<Local>) {
    match place {
        Place::Local(local) => {
            locals.push(*local);
        }
        Place::Field { base, .. } => {
            collect_place_reads(base, locals);
        }
        Place::Index { base, index, .. } => {
            collect_place_reads(base, locals);
            locals.push(*index); // The index variable is also read
        }
    }
}

/// Check if a statement has side effects that would prevent inlining.
fn has_side_effect(kind: &StatementKind<'_>, rvalue_reads: &HashSet<Local>) -> bool {
    match kind {
        StatementKind::Assign { destination, value } => {
            // Check if this assignment modifies a variable (or field/index of a variable)
            // that the rvalue reads from.
            let base_local = get_base_local(destination);
            if rvalue_reads.contains(&base_local) {
                return true;
            }
            // All other assignments (including loading constants) are pure
            _ = value;
            false
        }
        StatementKind::Drop(_) => true,
        StatementKind::Unwatch(_) => true, // Unwatch has side effects on watch graph
        StatementKind::NotifyBlock { .. } => true, // NotifyBlock has side effects (emits notification)
        StatementKind::WatchOptions { .. } => true, // WatchOptions has side effects on watch graph
        StatementKind::WatchNotify(_) => true, // WatchNotify has side effects (emits notification)
        StatementKind::VizEnter(_) | StatementKind::VizExit(_) => true, // VizEnter/VizExit emit notifications
        StatementKind::Nop => false,
    }
}

/// Get the base local from a place, following field/index projections.
fn get_base_local(place: &Place) -> Local {
    match place {
        Place::Local(local) => *local,
        Place::Field { base, .. } => get_base_local(base),
        Place::Index { base, .. } => get_base_local(base),
    }
}

/// Check if an rvalue can be inlined.
fn is_inlinable_rvalue(_rvalue: &Rvalue<'_>) -> bool {
    // All current rvalues can be inlined
    // May want to exclude complex aggregates in the future
    true
}

/// Check if an rvalue is a pure constant that can be safely duplicated.
///
/// Pure constants have no side effects and always produce the same value,
/// so they can be re-emitted at every use site even with multiple uses.
fn is_pure_constant(rvalue: &Rvalue<'_>) -> bool {
    matches!(rvalue, Rvalue::Use(Operand::Constant(_)))
}

/// Check if a local is a "call result immediate": defined by Call/Await/DispatchFuture,
/// used exactly once at the start of the continuation block.
///
/// Call result immediate applies when:
/// 1. The local is defined by a Call/Await/DispatchFuture terminator
/// 2. It has exactly one use
/// 3. The use is in the continuation block (target of the Call)
/// 4. The use is at statement index 0 (first thing in the continuation block)
///
/// This allows us to:
/// - After Call: don't emit `StoreVar` (leave result on stack)
/// - At use site: don't emit `LoadVar` (value already on stack from Call)
///
/// This eliminates the redundant `StoreVar("_X"); LoadVar("_X")` pattern for call results.
fn is_call_result_immediate(local: Local, du: &LocalDefUse<'_>, mir: &MirFunction<'_>) -> bool {
    // Must have exactly one use
    if du.uses.len() != 1 {
        return false;
    }

    // Must have a definition from a terminator (Call/Await/DispatchFuture)
    let Some(def) = &du.def else {
        return false;
    };

    // Definition must be in a terminator
    if def.statement_idx != TERMINATOR_IDX {
        return false;
    }

    let use_loc = &du.uses[0];

    // The use must be at the very start of the continuation block:
    // - statement index 0 (first statement), OR
    // - TERMINATOR_IDX if the block has no statements (use is directly in terminator)
    let use_block = mir.block(use_loc.block);
    let is_first_use = use_loc.statement_idx == 0
        || (use_loc.statement_idx == TERMINATOR_IDX && use_block.statements.is_empty());
    if !is_first_use {
        return false;
    }

    // Get the defining block and check that its terminator is Call/Await/DispatchFuture
    // with the continuation block being the use block
    let def_block = mir.block(def.block);
    let continuation_target = match &def_block.terminator {
        Some(Terminator::Call {
            destination,
            target,
            ..
        }) => {
            // Verify this Call defines our local
            if matches!(destination, Place::Local(l) if *l == local) {
                Some(*target)
            } else {
                None
            }
        }
        Some(Terminator::Await {
            destination,
            target,
            ..
        }) => {
            // Verify this Await defines our local
            if matches!(destination, Place::Local(l) if *l == local) {
                Some(*target)
            } else {
                None
            }
        }
        Some(Terminator::DispatchFuture { future, resume, .. }) => {
            // Verify this DispatchFuture defines our local
            if matches!(future, Place::Local(l) if *l == local) {
                Some(*resume)
            } else {
                None
            }
        }
        _ => None,
    };

    // Check that the continuation block is the use block
    continuation_target == Some(use_loc.block)
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
    local: Local,
    du: &LocalDefUse<'_>,
    mir: &MirFunction<'_>,
    all_defs: &HashMap<Local, Vec<(BlockId, usize)>>,
) -> Option<Local> {
    // Must have exactly one definition
    let def = du.def.as_ref()?;

    // Definition must not be from a terminator (Call/Await results aren't copies)
    if def.statement_idx == TERMINATOR_IDX {
        return None;
    }

    // Must have exactly one definition site
    let defs = all_defs.get(&local)?;
    if defs.len() != 1 {
        return None;
    }

    // The rvalue must be a simple copy/move of a local (not a field or index)
    let source = match &def.rvalue {
        Rvalue::Use(Operand::Copy(Place::Local(src))) => *src,
        Rvalue::Use(Operand::Move(Place::Local(src))) => *src,
        _ => return None,
    };

    // The source must be a parameter (parameters are never reassigned in MIR)
    // We only propagate copies of parameters to keep the analysis simple and safe.
    // Propagating copies of other locals would require verifying the source isn't
    // modified between the copy and all uses of the copy.
    let source_idx = source.0;
    if source_idx == 0 || source_idx > mir.arity {
        // Source is not a parameter (_0 is return value, > arity are locals)
        return None;
    }

    Some(source)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
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
}
