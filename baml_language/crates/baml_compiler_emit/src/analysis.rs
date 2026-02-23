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

use baml_compiler_mir::{
    BlockId, Constant, Local, MirFunction, Operand, Place, Rvalue, StatementKind, Terminator,
};

/// Optimization level for bytecode generation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OptLevel {
    /// No inlining of user-named locals. Compiler temps are still optimized.
    /// Produces bytecode that closely mirrors the source structure.
    Zero,
    /// Full optimization: inline single-use locals, copy propagation, stack carry.
    #[default]
    One,
}

use crate::stack_carry;

// ============================================================================
// Data Structures
// ============================================================================

/// A reference to either a statement or a terminator within a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    pub(crate) fn analyze(mir: &MirFunction, opt: OptLevel) -> Self {
        // Step 1: Build predecessor map
        let predecessors = build_predecessors(mir);

        // Step 2: Compute reverse postorder
        let rpo = compute_rpo(mir);

        // Step 3: Compute dominators
        let dominators = compute_dominators(mir, &rpo, &predecessors);

        // Step 4: Collect def-use information
        let def_use = collect_def_use(mir);

        // Step 5: Conservative jump threading (truly empty goto-only blocks).
        let initial_redirect_targets = build_redirect_targets(mir);

        // Step 6: First classification pass.
        let (mut classifications, mut copy_sources) = classify_locals(
            mir,
            &def_use,
            &dominators,
            &predecessors,
            &initial_redirect_targets,
            opt,
        );

        // Step 7: Enhanced jump threading using classification info.
        // Some blocks have statements that produce no bytecode (Virtual, Dead,
        // CopyOf assignments). These are effectively empty and can be threaded.
        let redirect_targets = build_redirect_targets_with_classifications(mir, &classifications);

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
                mir,
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

    /// Check if a block (after resolving redirects) has an Unreachable terminator.
    /// Used to optimize branches where the else path is unreachable (exhaustive matches).
    pub(crate) fn is_block_unreachable(&self, target: BlockId, mir: &MirFunction) -> bool {
        let resolved = self.resolve_jump_target(target);
        matches!(
            mir.block(resolved).terminator,
            Some(Terminator::Unreachable)
        )
    }
}

// ============================================================================
// CFG Analysis
// ============================================================================

/// Build predecessor map for all blocks.
fn build_predecessors(mir: &MirFunction) -> HashMap<BlockId, Vec<BlockId>> {
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
    mir: &MirFunction,
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
fn compute_rpo(mir: &MirFunction) -> Vec<BlockId> {
    let mut visited = HashSet::new();
    let mut postorder = Vec::new();

    rpo_dfs(mir, mir.entry, &mut visited, &mut postorder);
    postorder.reverse();
    postorder
}

// ============================================================================
// Emission Helpers
// ============================================================================

/// Check if a block is a "dead" unreachable block that may be skipped during
/// emission without changing observable behavior.
///
/// A block is dead if it has no statements and terminates with `Unreachable`.
pub(crate) fn is_dead_unreachable_block(block: &baml_compiler_mir::BasicBlock) -> bool {
    block.statements.is_empty() && matches!(block.terminator, Some(Terminator::Unreachable))
}

// ============================================================================
// Jump Threading
// ============================================================================

/// Build redirect targets for jump threading.
///
/// Identifies empty blocks that only contain a Goto terminator and maps them
/// to their final destination. This allows emission to skip intermediate jumps.
fn build_redirect_targets(mir: &MirFunction) -> HashMap<BlockId, BlockId> {
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

/// Build redirect targets using local classification info.
///
/// Like [`build_redirect_targets`] but also threads through blocks whose
/// statements all target locals classified as [`LocalClassification::Virtual`],
/// [`LocalClassification::Dead`], or [`LocalClassification::CopyOf`]. These
/// assignments produce no bytecode during emission, making the block
/// effectively empty.
fn build_redirect_targets_with_classifications(
    mir: &MirFunction,
    classifications: &HashMap<Local, LocalClassification>,
) -> HashMap<BlockId, BlockId> {
    let mut goto_targets: HashMap<BlockId, BlockId> = HashMap::new();

    for block in &mir.blocks {
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
    block: &baml_compiler_mir::BasicBlock,
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
    mir: &MirFunction,
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
fn collect_def_use(mir: &MirFunction) -> HashMap<Local, LocalDefUse> {
    let mut def_use: HashMap<Local, LocalDefUse> = HashMap::new();

    // Initialize for all locals
    for (idx, _) in mir.locals.iter().enumerate() {
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
    for block in &mir.blocks {
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
                    }

                    // Record uses in the rvalue
                    collect_uses_in_rvalue(value, block.id, stmt_ref, &mut def_use);
                }
                StatementKind::Drop(place) => {
                    collect_uses_in_place(place, block.id, stmt_ref, &mut def_use);
                }
                StatementKind::Unwatch(local) => {
                    // Unwatch uses the local (we need to read its value to unlink from watch graph)
                    def_use.get_mut(local).unwrap().uses.push(UseLocation {
                        block: block.id,
                        statement_ref: stmt_ref,
                    });
                }
                StatementKind::NotifyBlock { .. } => {
                    // NotifyBlock doesn't use any locals - it's a pure side effect
                }
                StatementKind::WatchOptions { local, filter } => {
                    // WatchOptions uses the local and the filter operand
                    def_use.get_mut(local).unwrap().uses.push(UseLocation {
                        block: block.id,
                        statement_ref: stmt_ref,
                    });
                    collect_uses_in_operand(filter, block.id, stmt_ref, &mut def_use);
                }
                StatementKind::WatchNotify(local) => {
                    // WatchNotify uses the local
                    def_use.get_mut(local).unwrap().uses.push(UseLocation {
                        block: block.id,
                        statement_ref: stmt_ref,
                    });
                }
                StatementKind::VizEnter(_) | StatementKind::VizExit(_) => {
                    // VizEnter/VizExit don't use any locals
                }
                StatementKind::Nop => {}
                StatementKind::Assert(operand) => {
                    // Assert uses the condition operand
                    collect_uses_in_operand(operand, block.id, stmt_ref, &mut def_use);
                }
            }
        }

        // Walk terminator
        if let Some(term) = &block.terminator {
            collect_uses_in_terminator(term, block.id, &mut def_use);
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
        Rvalue::Array(elements) => {
            for elem in elements {
                walk_operand_locals(elem, f);
            }
        }
        Rvalue::Map(entries) => {
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
        Rvalue::IsType { operand, .. } => walk_operand_locals(operand, f),
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
        Terminator::Switch { discriminant, .. } => {
            collect_uses_in_operand(discriminant, block, StatementRef::Terminator, def_use);
        }
        Terminator::Call {
            callee,
            args,
            destination,
            ..
        } => {
            collect_uses_in_operand(callee, block, StatementRef::Terminator, def_use);
            for arg in args {
                collect_uses_in_operand(arg, block, StatementRef::Terminator, def_use);
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
        Terminator::DispatchFuture {
            callee,
            args,
            future,
            ..
        } => {
            collect_uses_in_operand(callee, block, StatementRef::Terminator, def_use);
            for arg in args {
                collect_uses_in_operand(arg, block, StatementRef::Terminator, def_use);
            }
            // Record the def for the future place
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
    }
}

// ============================================================================
// Local Classification
// ============================================================================

/// Classify each local as Virtual, Real, `PhiLike`, `CopyOf`, or Dead.
///
/// Returns both the classifications and the `copy_sources` map for copy propagation.
fn classify_locals(
    mir: &MirFunction,
    def_use: &HashMap<Local, LocalDefUse>,
    dominators: &Dominators,
    predecessors: &HashMap<BlockId, Vec<BlockId>>,
    redirect_targets: &HashMap<BlockId, BlockId>,
    opt: OptLevel,
) -> (HashMap<Local, LocalClassification>, HashMap<Local, Local>) {
    let mut classifications = HashMap::new();
    let mut copy_sources: HashMap<Local, Local> = HashMap::new();
    let mut stack_carry_candidates: HashMap<Local, stack_carry::StackCarryKind> = HashMap::new();

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

        // User-named locals (name.is_some()) are kept as Real at O0.
        // Compiler temps have name=None and are always eligible for optimization.
        let is_user_local = local_decl.name.is_some();

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
        } else if idx != 0
            && let Some(source) = get_copy_source(du, mir, def_use)
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
        } else if can_be_virtual(du, dominators, mir, def_use, predecessors) {
            if opt == OptLevel::Zero && is_user_local {
                LocalClassification::Real
            } else {
                LocalClassification::Virtual
            }
        } else if is_phi_like(local, du, mir, predecessors, def_use) {
            // Stack-carry candidate validated in a later stack simulation pass.
            stack_carry_candidates.insert(local, stack_carry::StackCarryKind::PhiLike);
            LocalClassification::Real
        } else if is_return_phi(local, mir, def_use, redirect_targets) {
            // Stack-carry candidate validated in a later stack simulation pass.
            stack_carry_candidates.insert(local, stack_carry::StackCarryKind::ReturnPhi);
            LocalClassification::Real
        } else if is_call_result_immediate(local, du, mir) {
            // Stack-carry candidate validated in a later stack simulation pass.
            stack_carry_candidates.insert(local, stack_carry::StackCarryKind::CallResultImmediate);
            LocalClassification::Real
        } else {
            LocalClassification::Real
        };

        classifications.insert(local, classification);
    }

    stack_carry::refine_stack_carry_classifications(
        mir,
        def_use,
        &stack_carry_candidates,
        &mut classifications,
    );

    (classifications, copy_sources)
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
    du: &LocalDefUse,
    mir: &MirFunction,
    predecessors: &HashMap<BlockId, Vec<BlockId>>,
    def_use: &HashMap<Local, LocalDefUse>,
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
    let defs = &def_use[&local].all_defs;
    if defs.is_empty() {
        return false;
    }

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
            .any(|&(b, s)| b == pred_id && s == StatementRef::Statement(last_stmt_idx));
        if !has_def {
            return false;
        }
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
        StatementKind::Unwatch(_) => true,
        StatementKind::VizEnter(_) | StatementKind::VizExit(_) => true,
        StatementKind::NotifyBlock { .. } => true,
        StatementKind::WatchNotify(_) => true,
        StatementKind::Nop => true,

        // WatchOptions pushes 2 (channel, filter) then Watch pops 2 - net neutral
        // The return value stays at TOS throughout
        StatementKind::WatchOptions { .. } => true,

        // These modify the stack
        StatementKind::Assign { .. } => false,
        StatementKind::Drop(_) => false,
        // Assert pushes condition then pops it
        StatementKind::Assert(_) => false,
    }
}

/// Check if `_0` (the return place) is a "return-phi" local.
///
/// Return-phi applies when `_0` is assigned before Return in each defining block,
/// with only stack-neutral statements (like Unwatch, `VizExit`) between the assignment
/// and Return. This allows us to:
/// - At def sites: emit rvalue but NOT `StoreVar` (leave value on stack)
/// - At Return: skip `LoadVar` for _0 (value already on stack)
///
/// This eliminates the redundant `StoreVar("_0"); LoadVar("_0"); Return` pattern.
fn is_return_phi(
    local: Local,
    mir: &MirFunction,
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

            let block = mir.block(current);

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
        let block = mir.block(block_id);

        let stmt_idx = match stmt_ref {
            StatementRef::Terminator => {
                // For terminator definitions, check if the continuation leads to return safely
                let continuation = match &block.terminator {
                    Some(Terminator::Call { target, .. }) => Some(*target),
                    Some(Terminator::DispatchFuture { resume, .. }) => Some(*resume),
                    Some(Terminator::Await { target, .. }) => Some(*target),
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
    mir: &MirFunction,
    def_use: &HashMap<Local, LocalDefUse>,
    predecessors: &HashMap<BlockId, Vec<BlockId>>,
) -> bool {
    // Must have exactly one definition
    let Some(def) = &du.def else {
        return false;
    };

    // Definitions in terminators (Call/Await/DispatchFuture) cannot be inlined
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
                    mir,
                    def.block,
                    def_idx + 1,
                    mir.block(def.block).statements.len(),
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
                    mir,
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
                let is_param = read_local.0 > 0 && read_local.0 <= mir.arity;
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

        // Check if the use block is a loop header (has back-edge predecessors)
        // If so, be conservative and don't inline
        let use_preds = predecessors
            .get(&use_loc.block)
            .map_or(&[] as &[_], |v| v.as_slice());

        let has_back_edge = use_preds
            .iter()
            .any(|&pred| dominators.dominates(use_loc.block, pred));

        if has_back_edge {
            return false;
        }

        // Still check the def block (from def to end) and the use block
        // (from start to use) for same-block side effects.
        if has_side_effects_between(
            mir,
            def.block,
            def_idx + 1,
            mir.block(def.block).statements.len(),
            &def.rvalue,
            def_use,
        ) {
            return false;
        }

        if let StatementRef::Statement(use_idx) = use_loc.statement_ref {
            if use_idx > 0
                && has_side_effects_between(mir, use_loc.block, 0, use_idx, &def.rvalue, def_use)
            {
                return false;
            }
        }
    }

    true
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
        Rvalue::Array(elements) => elements.iter().any(operand_has_projection),
        Rvalue::Map(entries) => entries
            .iter()
            .any(|(key, value)| operand_has_projection(key) || operand_has_projection(value)),
        Rvalue::Aggregate { fields, .. } => fields.iter().any(operand_has_projection),
        Rvalue::Discriminant(place) | Rvalue::TypeTag(place) | Rvalue::Len(place) => {
            place_has_projection(place)
        }
        Rvalue::IsType { operand, .. } => operand_has_projection(operand),
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
    mir: &MirFunction,
    block_id: BlockId,
    start: usize,
    end: usize,
    rvalue: &Rvalue,
    def_use: &HashMap<Local, LocalDefUse>,
) -> bool {
    let block = mir.block(block_id);
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
        StatementKind::Assert(_) => true, // Assert has side effects (can throw)
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

/// Check if an rvalue is a pure constant that can be safely duplicated.
///
/// Pure constants have no side effects and always produce the same value,
/// so they can be re-emitted at every use site even with multiple uses.
fn is_pure_constant(rvalue: &Rvalue) -> bool {
    matches!(rvalue, Rvalue::Use(Operand::Constant(_)))
}

/// Check if a local is a "call result immediate": defined by Call/Await/DispatchFuture,
/// used exactly once in the continuation block.
///
/// Call result immediate applies when:
/// 1. The local is defined by a Call/Await/DispatchFuture terminator
/// 2. It has exactly one use
/// 3. The use is in the continuation block (target of the Call)
///
/// This allows us to:
/// - After Call: don't emit `StoreVar` (leave result on stack)
/// - At use site: don't emit `LoadVar` (value already on stack from Call)
///
/// This eliminates the redundant `StoreVar("_X"); LoadVar("_X")` pattern for call results.
fn is_call_result_immediate(local: Local, du: &LocalDefUse, mir: &MirFunction) -> bool {
    // Must have exactly one use
    if du.uses.len() != 1 {
        return false;
    }

    // Must have a definition from a terminator (Call/Await/DispatchFuture)
    let Some(def) = &du.def else {
        return false;
    };

    // Definition must be in a terminator
    if def.statement_ref != StatementRef::Terminator {
        return false;
    }

    // Get the defining block and check that its terminator is Call/Await/DispatchFuture
    // that defines this local.
    let def_block = mir.block(def.block);
    match &def_block.terminator {
        Some(Terminator::Call { destination, .. }) => {
            matches!(destination, Place::Local(l) if *l == local)
        }
        Some(Terminator::Await { destination, .. }) => {
            matches!(destination, Place::Local(l) if *l == local)
        }
        Some(Terminator::DispatchFuture { future, .. }) => {
            matches!(future, Place::Local(l) if *l == local)
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
    mir: &MirFunction,
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
    if source_idx == 0 || source_idx > mir.arity {
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
